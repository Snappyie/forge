// Package forge is a worker SDK for the Forge job scheduler.
//
// Standard library only. Three behaviours are load-bearing rather than
// cosmetic, and all three were absent before:
//
//   - The lease id returned by dequeue is captured and sent back on every
//     heartbeat and completion. Without it the server's CompletionGate is
//     unreachable: a completion after the lease expired returns 409, and the old
//     code discarded that error, so the work was re-dispatched and a side effect
//     silently applied twice.
//   - Failures carry an ErrorClass. The server records an absent one as
//     PERMANENT, which is not retryable, so every failure terminated the
//     execution and the platform's retry machinery was unreachable from Go.
//   - Errors are typed from the server's structured envelope, so a caller can
//     branch on the code instead of parsing a message.
//
// Contexts are respected throughout: a cancelled context stops the poll loop and
// releases the work, which is what a graceful shutdown needs.
package forge

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Error classes the server accepts. Mirrors forge_domain::ErrorClass; anything
// unrecognised is recorded as PERMANENT and therefore never retried.
const (
	ClassTransient           = "TRANSIENT"
	ClassDependencyUnavail   = "DEPENDENCY_UNAVAILABLE"
	ClassTimeout             = "TIMEOUT"
	ClassResourceExhausted   = "RESOURCE_EXHAUSTED"
	ClassRateLimited         = "RATE_LIMITED"
	ClassValidation          = "VALIDATION"
	ClassAuthentication      = "AUTHENTICATION"
	ClassAuthorization       = "AUTHORIZATION"
	ClassNotFound            = "NOT_FOUND"
	ClassConflict            = "CONFLICT"
	ClassCancellation        = "CANCELLATION"
	ClassPermanent           = "PERMANENT"
	ClassInternal            = "INTERNAL"
)

// Error is a failure reported by the Forge API.
//
// It carries the server's envelope rather than a bare status code, so a caller
// can branch on Code and an operator can quote RequestID in a bug report.
type Error struct {
	Message   string `json:"message"`
	Code      string `json:"code"`
	Status    int    `json:"status"`
	RequestID string `json:"request_id"`
}

func (e *Error) Error() string {
	if e.RequestID != "" {
		return fmt.Sprintf("%s (code=%s, request_id=%s)", e.Message, e.Code, e.RequestID)
	}
	return fmt.Sprintf("%s (code=%s)", e.Message, e.Code)
}

// Temporary reports whether retrying the same request could succeed.
//
// Used to back off on a 5xx or a dropped connection without giving up on a 4xx,
// which would fail identically however many times it was retried.
func (e *Error) Temporary() bool {
	return e.Status >= 500 || e.Status == 429 || e.Code == "DEPENDENCY_UNAVAILABLE"
}

// ErrLeaseLost means the lease for an execution expired or was reassigned while
// this worker held it.
//
// Distinct because the response is specific: stop working on it. Reporting
// success after losing the lease is how a side effect gets applied twice.
var ErrLeaseLost = errors.New("forge: lease lost; the execution is no longer owned by this worker")

// JobContext is what a handler is given for one execution.
type JobContext struct {
	ExecutionID string
	JobName     string
	Payload     map[string]interface{}
	// LeaseID accompanies the execution and must be sent back with every
	// heartbeat and completion.
	LeaseID string

	baseURL string
	token   string
	client  *http.Client
}

// Log streams a line back to Forge.
//
// Best-effort by design: a log line is never worth failing an execution over, so
// a transport error is reported to the caller rather than raised.
func (c *JobContext) Log(ctx context.Context, message string) error {
	fmt.Printf("[%s] %s\n", c.ExecutionID, message)
	return c.post(ctx, fmt.Sprintf("/executions/%s/logs", c.ExecutionID), map[string]interface{}{
		"stream":  "stdout",
		"message": message,
	})
}

// post issues one request for a log line.
//
// A context holds no worker token and no worker id of its own, so it cannot
// reuse Worker's transport; this is a deliberately small copy that shares the
// same error shape.
func (c *JobContext) post(ctx context.Context, path string, body interface{}) error {
	encoded, err := json.Marshal(body)
	if err != nil {
		return fmt.Errorf("forge: could not encode log: %w", err)
	}
	callCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()

	req, err := http.NewRequestWithContext(callCtx, http.MethodPost, c.baseURL+path, bytes.NewReader(encoded))
	if err != nil {
		return fmt.Errorf("forge: could not build log request: %w", err)
	}
	req.Header.Set("Authorization", "Bearer "+c.token)
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.client.Do(req)
	if err != nil {
		return &Error{Message: fmt.Sprintf("could not send log: %v", err), Code: "UNREACHABLE"}
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return &Error{
			Message: fmt.Sprintf("logging refused with HTTP %d", resp.StatusCode),
			Code:    "HTTP_ERROR",
			Status:  resp.StatusCode,
		}
	}
	return nil
}

// Logf is Log with formatting.
func (c *JobContext) Logf(ctx context.Context, format string, args ...interface{}) error {
	return c.Log(ctx, fmt.Sprintf(format, args...))
}

// JobHandler runs one execution. A returned error is reported to the server with
// a classification derived from its type.
type JobHandler func(ctx context.Context, jobCtx *JobContext) (interface{}, error)

// Worker polls a queue and runs the registered handlers.
//
// Typical use:
//
//	w := forge.NewWorker(baseURL, tenantID, adminToken)
//	if err := w.RegisterWorker(ctx, "worker-1", hostname, nil); err != nil { ... }
//	w.Register("settle", settle)
//	err := w.Start(ctx, "critical", 2*time.Second)
type Worker struct {
	BaseURL  string
	TenantID string
	// APIKey is replaced by the worker token when RegisterWorker succeeds. A
	// worker token carries only workers:claim, workers:heartbeat and
	// executions:write, so it must never be used for administration.
	APIKey string
	// WorkerID is what the worker registered under. A worker credential is bound
	// to exactly one worker, so it must present that id when dequeuing.
	WorkerID string

	client   *http.Client
	handlers map[string]JobHandler

	mu      sync.Mutex
	stopped bool
}

// NewWorker builds a worker against a Forge API base URL such as
// "http://localhost:3000/api/v1".
func NewWorker(baseURL, tenantID, apiKey string) *Worker {
	return &Worker{
		BaseURL:  strings.TrimRight(baseURL, "/"),
		TenantID: tenantID,
		APIKey:   apiKey,
		WorkerID: "go-worker-1",
		client: &http.Client{
			// Every call is bounded: an unbounded request hangs the worker
			// goroutine forever and `Start` can then never be interrupted.
			Timeout: 15 * time.Second,
		},
		handlers: make(map[string]JobHandler),
	}
}

// WithWorkerID binds the worker to an id it is already registered under.
func (w *Worker) WithWorkerID(workerID string) *Worker {
	w.WorkerID = workerID
	return w
}

// Register associates a handler with a job name.
func (w *Worker) Register(jobName string, handler JobHandler) {
	w.handlers[jobName] = handler
}

// RegisterWorker registers with the server, acquiring a worker id and token.
//
// Needs an operator credential: a worker token cannot register itself.
func (w *Worker) RegisterWorker(ctx context.Context, name, hostname string, capabilities []string) error {
	if capabilities == nil {
		capabilities = []string{"*"}
	}
	data, err := w.post(ctx, "/workers/register", map[string]interface{}{
		"name":         name,
		"hostname":     hostname,
		"capabilities": capabilities,
	}, 15*time.Second)
	if err != nil {
		return fmt.Errorf("forge: could not register worker: %w", err)
	}
	if id, ok := data["id"].(string); ok && id != "" {
		w.WorkerID = id
	}
	if token, ok := data["token"].(string); ok && token != "" {
		w.APIKey = token
	}
	return nil
}

// PollOnce claims a single execution, or returns nil when the queue is empty.
//
// Exposed because the poll loop is not always what a caller wants: a serverless
// or event-driven worker wants one claim per invocation, and a test wants to
// drive the protocol deterministically rather than racing a loop.
func (w *Worker) PollOnce(ctx context.Context, queue string) (map[string]interface{}, error) {
	if w.isStopped() {
		return nil, context.Canceled
	}
	return w.dequeue(ctx, queue)
}

// RunOne executes a claimed execution to completion and reports its outcome.
//
// Separated from the poll loop for the same reason as PollOnce: the caller
// decides the lifecycle, and a test can assert on what was reported.
func (w *Worker) RunOne(ctx context.Context, execution map[string]interface{}) {
	w.execute(ctx, execution)
}

// Start polls the queue until ctx is cancelled.
//
// The context is the shutdown mechanism: cancelling it stops the loop, ends the
// worker heartbeat, and cancels any in-flight request, so a handler is never
// left running against a connection the process is about to drop.
func (w *Worker) Start(ctx context.Context, queue string, pollInterval time.Duration) error {
	if len(w.handlers) == 0 {
		return errors.New("forge: no handlers registered; call Register first")
	}
	fmt.Printf("Forge worker %s listening on queue %q for %d handler(s)\n",
		w.WorkerID, queue, len(w.handlers))

	workerCtx, cancel := context.WithCancel(ctx)
	defer cancel()
	go w.workerHeartbeatLoop(workerCtx)

	ticker := time.NewTicker(pollInterval)
	defer ticker.Stop()

	for {
		// Check the context first: otherwise a cancelled worker runs one more
		// full poll interval before noticing.
		select {
		case <-workerCtx.Done():
			fmt.Println("Forge worker stopping")
			return workerCtx.Err()
		case <-ticker.C:
		}

		execution, err := w.dequeue(workerCtx, queue)
		if err != nil {
			var apiErr *Error
			if errors.As(err, &apiErr) && (apiErr.Status == http.StatusUnauthorized || apiErr.Status == http.StatusForbidden) {
				// A credential problem will not fix itself by polling harder,
				// and a tight loop against it buries the reason.
				fmt.Printf("forge: refusing to poll with this credential: %v\n", apiErr)
				return apiErr
			}
			fmt.Printf("forge: error polling queue %s: %v\n", queue, err)
			continue
		}
		if execution == nil {
			continue
		}
		w.execute(workerCtx, execution)
	}
}

// Stop ends the worker without waiting for a context to be cancelled.
func (w *Worker) Stop() {
	w.mu.Lock()
	w.stopped = true
	w.mu.Unlock()
}

func (w *Worker) isStopped() bool {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.stopped
}

// dequeue claims one execution, keeping the lease id it returns.
//
// The lease travels with the work. Discarding it is what made a completion after
// lease expiry an unrecoverable 409 and the source of duplicate side effects.
func (w *Worker) dequeue(ctx context.Context, queue string) (map[string]interface{}, error) {
	data, err := w.post(ctx, fmt.Sprintf("/queues/%s/dequeue", queue),
		map[string]interface{}{"worker_id": w.WorkerID}, 20*time.Second)
	if err != nil {
		return nil, err
	}
	if len(data) == 0 {
		return nil, nil
	}
	return data, nil
}

// workerHeartbeatLoop keeps the worker dispatchable.
//
// The server marks a worker OFFLINE after a period of silence and an offline
// worker is filtered out of dispatch, so without this a long-lived worker goes
// quiet and silently stops being given work.
func (w *Worker) workerHeartbeatLoop(ctx context.Context) {
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		if _, err := w.post(ctx,
			fmt.Sprintf("/workers/%s/heartbeat", w.WorkerID),
			map[string]interface{}{}, 5*time.Second); err != nil {
			if ctx.Err() != nil {
				return
			}
		}
	}
}

// executionHeartbeatLoop renews the lease while an execution runs.
//
// The lease id is sent so the server can reject a heartbeat for a lease that has
// already been reassigned, rather than renewing someone else's.
func (w *Worker) executionHeartbeatLoop(ctx context.Context, executionID, leaseID string) {
	ticker := time.NewTicker(10 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		_, _ = w.post(ctx, fmt.Sprintf("/executions/%s/heartbeat", executionID),
			map[string]interface{}{"worker_id": w.WorkerID, "lease_id": leaseID}, 5*time.Second)
	}
}

func (w *Worker) execute(parent context.Context, execution map[string]interface{}) {
	executionID, _ := execution["id"].(string)
	if executionID == "" {
		return
	}
	jobName, _ := execution["job_name"].(string)
	if jobName == "" {
		jobName, _ = execution["type"].(string)
	}
	leaseID, _ := execution["lease_id"].(string)

	payload, ok := execution["input"].(map[string]interface{})
	if !ok {
		payload = map[string]interface{}{}
	}

	handler, exists := w.handlers[jobName]
	if !exists {
		_ = w.fail(parent, executionID, leaseID,
			fmt.Sprintf("no handler registered for job %q", jobName), ClassNotFound, "")
		return
	}

	// The handler runs under a context derived from the worker, so a shutdown
	// cancels in-flight work instead of abandoning it on a dead connection.
	ctx, cancel := context.WithCancel(parent)
	defer cancel()

	jobCtx := &JobContext{
		ExecutionID: executionID,
		JobName:     jobName,
		Payload:     payload,
		LeaseID:     leaseID,
		baseURL:     w.BaseURL,
		token:       w.APIKey,
		client:      w.client,
	}

	heartbeatCtx, stopHeartbeat := context.WithCancel(ctx)
	go w.executionHeartbeatLoop(heartbeatCtx, executionID, leaseID)

	_ = jobCtx.Logf(ctx, "starting %s", jobName)
	result, err := handler(ctx, jobCtx)

	stopHeartbeat()
	cancel()

	if err != nil {
		if errors.Is(err, context.Canceled) && parent.Err() != nil {
			// Shutting down mid-execution. Reporting nothing leaves the lease to
			// expire and the work to be retried, which is the correct outcome:
			// reporting a failure would look like the work had been attempted.
			_ = jobCtx.Logf(ctx, "abandoning %s: worker shutting down", jobName)
			return
		}
		_ = jobCtx.Logf(ctx, "%s failed: %v", jobName, err)
		_ = w.fail(parent, executionID, leaseID, err.Error(), Classify(err), traceOf(err))
		return
	}

	if err := w.Complete(parent, executionID, leaseID, result); err != nil {
		if errors.Is(err, ErrLeaseLost) {
			// The lease is gone, so this worker no longer owns the execution.
			// Reporting success would apply the side effect a second time.
			_ = jobCtx.Logf(ctx, "lease lost during %s; abandoning without reporting an outcome", jobName)
			return
		}
		return
	}
	_ = jobCtx.Logf(ctx, "completed %s", jobName)
}

// Complete reports a successful execution.
//
// Returns ErrLeaseLost rather than a generic error when the lease has gone, so
// the caller can distinguish "someone else owns this now" from "the network
// broke" and stop rather than retry.
func (w *Worker) Complete(ctx context.Context, executionID, leaseID string, output interface{}) error {
	_, err := w.post(ctx, fmt.Sprintf("/executions/%s/complete", executionID), map[string]interface{}{
		"worker_id": w.WorkerID,
		"lease_id":  leaseID,
		"succeeded": true,
		"output":    output,
	}, 15*time.Second)
	if err != nil {
		var apiErr *Error
		if errors.As(err, &apiErr) && apiErr.Status == http.StatusConflict {
			return ErrLeaseLost
		}
	}
	return err
}

// Fail reports a failed execution with a classification.
//
// ErrorClass is the field that decides whether the execution is retried;
// omitting it makes every failure PERMANENT, which is what made retries
// unreachable from Go.
func (w *Worker) Fail(ctx context.Context, executionID, leaseID, message, class, trace string) error {
	return w.fail(ctx, executionID, leaseID, message, class, trace)
}

func (w *Worker) fail(ctx context.Context, executionID, leaseID, message, class, trace string) error {
	body := map[string]interface{}{
		"worker_id":   w.WorkerID,
		"lease_id":    leaseID,
		"succeeded":   false,
		"error":       message,
		"error_class": class,
	}
	if trace != "" {
		body["trace"] = trace
	}
	_, err := w.post(ctx, fmt.Sprintf("/executions/%s/fail", executionID), body, 15*time.Second)
	if err != nil {
		var apiErr *Error
		// A lost lease is not a failure to report; the work will be retried.
		if errors.As(err, &apiErr) && apiErr.Status == http.StatusConflict {
			return nil
		}
	}
	return err
}

// Classify picks an error class for an error a handler did not classify.
//
// Getting this roughly right is what makes the platform's retry policy usable
// from Go: an unreachable dependency or a timeout is worth another attempt,
// while bad input is not.
func Classify(err error) string {
	if err == nil {
		return ClassSuccess
	}
	switch {
	case errors.Is(err, context.DeadlineExceeded):
		return ClassTimeout
	case errors.Is(err, context.Canceled):
		return ClassCancellation
	}

	var netErr net.Error
	if errors.As(err, &netErr) && netErr.Timeout() {
		return ClassTimeout
	}
	if errors.Is(err, ErrLeaseLost) {
		return ClassConflict
	}

	var apiErr *Error
	if errors.As(err, &apiErr) {
		// The server already classified it; keep its answer.
		if apiErr.Code != "" {
			return apiErr.Code
		}
		switch apiErr.Status {
		case http.StatusBadRequest:
			return ClassValidation
		case http.StatusUnauthorized:
			return ClassAuthentication
		case http.StatusForbidden:
			return ClassAuthorization
		case http.StatusNotFound:
			return ClassNotFound
		case http.StatusConflict:
			return ClassConflict
		case http.StatusTooManyRequests:
			return ClassRateLimited
		default:
			if apiErr.Temporary() {
				return ClassTransient
			}
		}
	}

	// A malformed number or a malformed body is the caller's mistake, not
	// something a retry can fix.
	var syntaxErr *strconv.NumError
	if errors.As(err, &syntaxErr) {
		return ClassValidation
	}
	var jsonErr *json.SyntaxError
	if errors.As(err, &jsonErr) {
		return ClassValidation
	}
	if errors.Is(err, io.ErrUnexpectedEOF) {
		return ClassTransient
	}
	// Optimistic on purpose: retrying a wrongly-classified failure is
	// recoverable, while not retrying a transient one silently loses work.
	return ClassTransient
}

// ClassSuccess is reported when a handler returns no error.
const ClassSuccess = "SUCCESS"

func traceOf(err error) string {
	if err == nil {
		return ""
	}
	// The server stores `trace` but does not surface it, so what is useful here
	// is the error's own chain, which at least names the failing step.
	return fmt.Sprintf("%+v", err)
}

// post issues one request and unwraps the standard envelope.
//
// Every call goes through here, which makes "every request has a timeout" and
// "errors are structured" properties of the SDK rather than things each call
// site has to remember.
func (w *Worker) post(ctx context.Context, path string, body interface{}, timeout time.Duration) (map[string]interface{}, error) {
	var reader io.Reader
	if body != nil {
		encoded, err := json.Marshal(body)
		if err != nil {
			return nil, fmt.Errorf("forge: could not encode request: %w", err)
		}
		reader = bytes.NewReader(encoded)
	}

	callCtx := ctx
	if timeout > 0 {
		var cancel context.CancelFunc
		callCtx, cancel = context.WithTimeout(ctx, timeout)
		defer cancel()
	}

	req, err := http.NewRequestWithContext(callCtx, http.MethodPost, w.BaseURL+path, reader)
	if err != nil {
		return nil, fmt.Errorf("forge: could not build request: %w", err)
	}
	req.Header.Set("Authorization", "Bearer "+w.APIKey)
	req.Header.Set("Content-Type", "application/json")

	resp, err := w.client.Do(req)
	if err != nil {
		// A transport failure is temporary by definition: the server may be
		// back in a moment. Reporting it as permanent makes a caller stop
		// retrying a request that would have succeeded, and reporting it as
		// fatal makes one spin against a server that is merely restarting.
		apiErr := &Error{
			Message: fmt.Sprintf("could not reach Forge at %s: %v", path, err),
			Code:    "UNREACHABLE",
		}
		if errors.Is(err, context.DeadlineExceeded) || os.IsTimeout(err) {
			apiErr.Status = http.StatusGatewayTimeout
		} else if ctx.Err() != nil {
			apiErr.Status = http.StatusRequestTimeout
		} else {
			apiErr.Status = http.StatusServiceUnavailable
		}
		return nil, apiErr
	}
	defer resp.Body.Close()

	raw, readErr := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if resp.StatusCode >= 400 {
		apiErr := &Error{Status: resp.StatusCode, Code: "HTTP_ERROR"}
		var envelope struct {
			Error struct {
				Message   string `json:"message"`
				Code      string `json:"code"`
				RequestID string `json:"request_id"`
			} `json:"error"`
		}
		if json.Unmarshal(raw, &envelope) == nil && envelope.Error.Message != "" {
			apiErr.Message = envelope.Error.Message
			apiErr.Code = envelope.Error.Code
			apiErr.RequestID = envelope.Error.RequestID
		} else {
			apiErr.Message = fmt.Sprintf("Forge returned HTTP %d for %s", resp.StatusCode, path)
		}
		return nil, apiErr
	}
	if readErr != nil {
		return nil, fmt.Errorf("forge: could not read response from %s: %w", path, readErr)
	}

	var envelope struct {
		Data map[string]interface{} `json:"data"`
	}
	if err := json.Unmarshal(raw, &envelope); err != nil {
		return nil, &Error{
			Message: fmt.Sprintf("could not decode the response from %s: %v", path, err),
			Code:    "BAD_RESPONSE",
			Status:  resp.StatusCode,
		}
	}
	if envelope.Data == nil {
		return map[string]interface{}{}, nil
	}
	return envelope.Data, nil
}
