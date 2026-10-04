package forge

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

type JobContext struct {
	ExecutionID string
	Payload     map[string]interface{}
	baseURL     string
	apiKey      string
	client      *http.Client
}

func (ctx *JobContext) Log(message string) {
	fmt.Printf("[%s] %s\n", ctx.ExecutionID, message)
	body, _ := json.Marshal(map[string]string{"message": message})
	req, _ := http.NewRequest("POST", fmt.Sprintf("%s/executions/%s/logs", ctx.baseURL, ctx.ExecutionID), bytes.NewBuffer(body))
	req.Header.Set("Authorization", "Bearer "+ctx.apiKey)
	req.Header.Set("Content-Type", "application/json")
	ctx.client.Do(req)
}

type JobHandler func(ctx *JobContext) (interface{}, error)

type Worker struct {
	BaseURL  string
	TenantID string
	APIKey   string
	// WorkerID is the id returned by POST /workers/register. The server binds a
	// worker credential to exactly one worker, so a worker token must present its
	// own id when dequeuing. Left empty it falls back to a stable name, which is
	// only accepted from an operator credential that may register the worker on
	// the spot.
	WorkerID string
	client   *http.Client
	handlers map[string]JobHandler
}

func NewWorker(baseURL, tenantID, apiKey string) *Worker {
	return &Worker{
		BaseURL:  baseURL,
		TenantID: tenantID,
		APIKey:   apiKey,
		WorkerID: "go-worker-1",
		client:   &http.Client{Timeout: 10 * time.Second},
		handlers: make(map[string]JobHandler),
	}
}

// WithWorkerID binds the worker to the id it registered with.
func (w *Worker) WithWorkerID(workerID string) *Worker {
	w.WorkerID = workerID
	return w
}

func (w *Worker) Register(jobName string, handler JobHandler) {
	w.handlers[jobName] = handler
}

func (w *Worker) Start(queue string, pollInterval time.Duration) {
	fmt.Printf("ForgeWorker started. Listening on queue '%s'\n", queue)

	// A worker that stops reporting is marked OFFLINE and receives no more work
	// (spec 01.10, spec 10.10), so the worker itself must heartbeat, not only
	// the executions it is running.
	go w.workerHeartbeatLoop()

	for {
		w.poll(queue)
		time.Sleep(pollInterval)
	}
}

// workerHeartbeatLoop keeps the worker dispatchable.
//
// The server marks a worker OFFLINE after 90 seconds of silence, and an offline
// worker is filtered out of dispatch, so without this a long-lived worker goes
// quiet and silently stops being given work.
func (w *Worker) workerHeartbeatLoop() {
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		req, err := http.NewRequest("POST",
			fmt.Sprintf("%s/workers/%s/heartbeat", w.BaseURL, w.WorkerID), nil)
		if err != nil {
			continue
		}
		req.Header.Set("Authorization", "Bearer "+w.APIKey)
		resp, err := w.client.Do(req)
		if err == nil {
			resp.Body.Close()
		}
	}
}

func (w *Worker) poll(queue string) {
	body, _ := json.Marshal(map[string]string{"worker_id": w.WorkerID})
	req, _ := http.NewRequest("POST", fmt.Sprintf("%s/queues/%s/dequeue", w.BaseURL, queue), bytes.NewBuffer(body))
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")

	resp, err := w.client.Do(req)
	if err != nil || resp.StatusCode != http.StatusOK {
		return
	}
	defer resp.Body.Close()

	respBody, _ := io.ReadAll(resp.Body)
	// Every success is wrapped: {"data": ..., "request_id": ...}
	var envelope struct {
		Data map[string]interface{} `json:"data"`
	}
	if err := json.Unmarshal(respBody, &envelope); err != nil || len(envelope.Data) == 0 {
		return
	}

	go w.executeJob(envelope.Data)
}

func (w *Worker) executeJob(execution map[string]interface{}) {
	executionID, _ := execution["id"].(string)
	if executionID == "" {
		return
	}
	jobName, ok := execution["job_name"].(string)
	if !ok {
		jobName, _ = execution["type"].(string)
	}
	payload, ok := execution["input"].(map[string]interface{})
	if !ok {
		payload = make(map[string]interface{})
	}

	handler, exists := w.handlers[jobName]
	if !exists {
		w.failJob(executionID, fmt.Sprintf("No handler registered for job: %s", jobName), "")
		return
	}

	ctx := &JobContext{
		ExecutionID: executionID,
		Payload:     payload,
		baseURL:     w.BaseURL,
		apiKey:      w.APIKey,
		client:      w.client,
	}

	done := make(chan bool)
	go w.heartbeat(executionID, done)

	ctx.Log("Starting execution of " + jobName)
	result, err := handler(ctx)

	close(done)

	if err != nil {
		ctx.Log("Execution failed: " + err.Error())
		w.failJob(executionID, err.Error(), "")
	} else {
		w.completeJob(executionID, result)
		ctx.Log("Successfully completed " + jobName)
	}
}

// RegisterWorker registers the worker with Forge and acquires its worker_id and worker token.
func (w *Worker) RegisterWorker(name, hostname string, capabilities []string) error {
	if capabilities == nil {
		capabilities = []string{"*"}
	}
	body, _ := json.Marshal(map[string]interface{}{
		"name":         name,
		"hostname":     hostname,
		"capabilities": capabilities,
	})
	req, err := http.NewRequest("POST", fmt.Sprintf("%s/workers/register", w.BaseURL), bytes.NewBuffer(body))
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := w.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusCreated {
		return fmt.Errorf("failed to register worker: status %d", resp.StatusCode)
	}
	var env struct {
		Data struct {
			ID    string `json:"id"`
			Token string `json:"token"`
		} `json:"data"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&env); err != nil {
		return err
	}
	if env.Data.ID != "" {
		w.WorkerID = env.Data.ID
	}
	if env.Data.Token != "" {
		w.APIKey = env.Data.Token
	}
	return nil
}

func (w *Worker) heartbeat(executionID string, done chan bool) {
	ticker := time.NewTicker(15 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-done:
			return
		case <-ticker.C:
			body, _ := json.Marshal(map[string]string{"worker_id": w.WorkerID})
			req, _ := http.NewRequest("POST", fmt.Sprintf("%s/executions/%s/heartbeat", w.BaseURL, executionID), bytes.NewBuffer(body))
			req.Header.Set("Authorization", "Bearer "+w.APIKey)
			req.Header.Set("Content-Type", "application/json")
			w.client.Do(req)
		}
	}
}

func (w *Worker) failJob(executionID, errMsg, trace string) {
	body, _ := json.Marshal(map[string]string{"worker_id": w.WorkerID, "error": errMsg, "trace": trace})
	req, _ := http.NewRequest("POST", fmt.Sprintf("%s/executions/%s/fail", w.BaseURL, executionID), bytes.NewBuffer(body))
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")
	w.client.Do(req)
}

func (w *Worker) completeJob(executionID string, output interface{}) {
	body, _ := json.Marshal(map[string]interface{}{"worker_id": w.WorkerID, "output": output})
	req, _ := http.NewRequest("POST", fmt.Sprintf("%s/executions/%s/complete", w.BaseURL, executionID), bytes.NewBuffer(body))
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")
	w.client.Do(req)
}
