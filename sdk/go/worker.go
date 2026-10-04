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
	client   *http.Client
	handlers map[string]JobHandler
}

func NewWorker(baseURL, tenantID, apiKey string) *Worker {
	return &Worker{
		BaseURL:  baseURL,
		TenantID: tenantID,
		APIKey:   apiKey,
		client:   &http.Client{Timeout: 10 * time.Second},
		handlers: make(map[string]JobHandler),
	}
}

func (w *Worker) Register(jobName string, handler JobHandler) {
	w.handlers[jobName] = handler
}

func (w *Worker) Start(queue string, pollInterval time.Duration) {
	fmt.Printf("ForgeWorker started. Listening on queue '%s'\n", queue)

	for {
		w.poll(queue)
		time.Sleep(pollInterval)
	}
}

func (w *Worker) poll(queue string) {
	body, _ := json.Marshal(map[string]string{"worker_id": "go-worker-1"})
	req, _ := http.NewRequest("POST", fmt.Sprintf("%s/queues/%s/dequeue", w.BaseURL, queue), bytes.NewBuffer(body))
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")

	resp, err := w.client.Do(req)
	if err != nil || resp.StatusCode != http.StatusOK {
		return
	}
	defer resp.Body.Close()

	respBody, _ := io.ReadAll(resp.Body)
	var execution map[string]interface{}
	if err := json.Unmarshal(respBody, &execution); err != nil || len(execution) == 0 {
		return
	}

	go w.executeJob(execution)
}

func (w *Worker) executeJob(execution map[string]interface{}) {
	executionID := execution["id"].(string)
	jobName, ok := execution["job_name"].(string)
	if !ok {
		jobName, _ = execution["type"].(string)
	}
	payload, ok := execution["payload"].(map[string]interface{})
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

func (w *Worker) heartbeat(executionID string, done chan bool) {
	ticker := time.NewTicker(15 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-done:
			return
		case <-ticker.C:
			req, _ := http.NewRequest("PATCH", fmt.Sprintf("%s/executions/%s/heartbeat", w.BaseURL, executionID), nil)
			req.Header.Set("Authorization", "Bearer "+w.APIKey)
			w.client.Do(req)
		}
	}
}

func (w *Worker) failJob(executionID, errMsg, trace string) {
	body, _ := json.Marshal(map[string]string{"error": errMsg, "trace": trace})
	req, _ := http.NewRequest("POST", fmt.Sprintf("%s/executions/%s/fail", w.BaseURL, executionID), bytes.NewBuffer(body))
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")
	w.client.Do(req)
}

func (w *Worker) completeJob(executionID string, output interface{}) {
	body, _ := json.Marshal(map[string]interface{}{"output": output})
	req, _ := http.NewRequest("POST", fmt.Sprintf("%s/executions/%s/complete", w.BaseURL, executionID), bytes.NewBuffer(body))
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")
	w.client.Do(req)
}
