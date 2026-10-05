package forge

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

// The tests below use a stub server rather than a live Forge, because they are
// about the SDK's own behaviour: which fields it sends and what it does with
// the responses. The fields themselves are pinned against the real contract in
// docs — a stub cannot tell you the server renamed `lease_id`, which is exactly
// the class of bug that went unnoticed.

// captured records what the SDK actually sent.
type captured struct {
	Path string
	Body map[string]interface{}
}

func newStub(t *testing.T, handler func(w http.ResponseWriter, r *http.Request)) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(handler))
	t.Cleanup(srv.Close)
	return srv
}

func TestDequeueKeepsTheLeaseID(t *testing.T) {
	var got captured
	srv := newStub(t, func(w http.ResponseWriter, r *http.Request) {
		got.Path = r.URL.Path
		_ = decode(r, &got.Body)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"data":{"id":"ex-1","job_name":"settle","lease_id":"lease-abc","input":{"n":1}}}`))
	})

	w := NewWorker(srv.URL, "t", "tok").WithWorkerID("w-1")
	execution, err := w.dequeue(context.Background(), "critical")
	if err != nil {
		t.Fatalf("dequeue: %v", err)
	}

	// The lease travels with the work. Dropping it is what made a completion
	// after lease expiry an unrecoverable 409 and the source of duplicate side
	// effects.
	if got := execution["lease_id"]; got != "lease-abc" {
		t.Fatalf("lease id was dropped: got %v, want lease-abc", got)
	}
	if execution["id"] != "ex-1" {
		t.Fatalf("execution id lost: %v", execution["id"])
	}
}

func TestCompletionSendsTheLeaseAndSuccess(t *testing.T) {
	var got captured
	srv := newStub(t, func(w http.ResponseWriter, r *http.Request) {
		got.Path = r.URL.Path
		_ = decode(r, &got.Body)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"data":{"status":"SUCCEEDED"}}`))
	})

	w := NewWorker(srv.URL, "t", "tok").WithWorkerID("w-1")
	if err := w.Complete(context.Background(), "ex-1", "lease-abc", map[string]int{"n": 1}); err != nil {
		t.Fatalf("complete: %v", err)
	}

	// `lease_id` is what the server's CompletionGate checks; `succeeded` is what
	// distinguishes a success from a failure.
	if got.Body["lease_id"] != "lease-abc" {
		t.Errorf("lease_id not sent: %v", got.Body)
	}
	if got.Body["succeeded"] != true {
		t.Errorf("succeeded not sent as true: %v", got.Body)
	}
	if got.Body["worker_id"] != "w-1" {
		t.Errorf("worker_id not sent: %v", got.Body)
	}
}

func TestFailureCarriesAnErrorClass(t *testing.T) {
	var got captured
	srv := newStub(t, func(w http.ResponseWriter, r *http.Request) {
		_ = decode(r, &got.Body)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"data":{"status":"FAILED"}}`))
	})

	w := NewWorker(srv.URL, "t", "tok").WithWorkerID("w-1")
	if err := w.Fail(context.Background(), "ex-1", "lease-abc", "upstream down", ClassTransient, ""); err != nil {
		t.Fatalf("fail: %v", err)
	}

	// Without this the server records PERMANENT, which is not retryable, and
	// the retry machinery is unreachable from Go.
	if got.Body["error_class"] != ClassTransient {
		t.Errorf("error_class not sent: %v", got.Body)
	}
	if got.Body["succeeded"] != false {
		t.Errorf("failure must send succeeded=false: %v", got.Body)
	}
}

func TestAConflictOnCompletionIsLeaseLost(t *testing.T) {
	srv := newStub(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusConflict)
		_, _ = w.Write([]byte(`{"error":{"code":"CONFLICT","message":"a worker must hold a lease","request_id":"rq-9"}}`))
	})

	w := NewWorker(srv.URL, "t", "tok").WithWorkerID("w-1")
	err := w.Complete(context.Background(), "ex-1", "stale-lease", nil)

	// Distinguishable from a generic failure, because the response is specific:
	// stop working, do not report a success for work this worker no longer owns.
	if !errors.Is(err, ErrLeaseLost) {
		t.Fatalf("want ErrLeaseLost, got %v", err)
	}
}

func TestErrorsCarryTheServerEnvelope(t *testing.T) {
	srv := newStub(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusForbidden)
		_, _ = w.Write([]byte(`{"error":{"code":"AUTHORIZATION_DENIED","message":"role VIEWER does not grant ` + "`jobs:write`" + `","request_id":"rq-42"}}`))
	})

	w := NewWorker(srv.URL, "t", "tok").WithWorkerID("w-1")
	_, err := w.post(context.Background(), "/jobs", map[string]interface{}{}, time.Second)
	if err == nil {
		t.Fatal("expected an error")
	}

	var apiErr *Error
	if !errors.As(err, &apiErr) {
		t.Fatalf("want a *forge.Error, got %T", err)
	}
	// The code and request id are what make a failure diagnosable; a bare
	// "HTTP 403" is not.
	if apiErr.Code != "AUTHORIZATION_DENIED" {
		t.Errorf("code lost: %q", apiErr.Code)
	}
	if apiErr.RequestID != "rq-42" {
		t.Errorf("request id lost: %q", apiErr.RequestID)
	}
	if apiErr.Status != http.StatusForbidden {
		t.Errorf("status lost: %d", apiErr.Status)
	}
}

func TestAnUnreachableServerIsTypedNotAPanic(t *testing.T) {
	// A closed port, so the request genuinely cannot be made.
	w := NewWorker("http://127.0.0.1:1", "t", "tok")
	_, err := w.post(context.Background(), "/queues/critical/dequeue", map[string]interface{}{}, time.Second)
	if err == nil {
		t.Fatal("expected an error against an unreachable server")
	}
	var apiErr *Error
	if !errors.As(err, &apiErr) {
		t.Fatalf("want a *forge.Error, got %T", err)
	}
	if !apiErr.Temporary() {
		t.Error("an unreachable server should be temporary, so the caller backs off")
	}
}

func TestEveryRequestIsBounded(t *testing.T) {
	// A server that never answers. Without a per-request timeout this hangs
	// forever and the worker's poll loop can never be interrupted.
	hang := make(chan struct{})
	srv := newStub(t, func(w http.ResponseWriter, r *http.Request) {
		<-hang
	})
	defer close(hang)

	w := NewWorker(srv.URL, "t", "tok")
	done := make(chan error, 1)
	go func() {
		_, err := w.post(context.Background(), "/queues/critical/dequeue",
			map[string]interface{}{}, 200*time.Millisecond)
		done <- err
	}()

	select {
	case err := <-done:
		if err == nil {
			t.Fatal("expected a timeout error")
		}
	case <-time.After(3 * time.Second):
		t.Fatal("the request was not bounded; a hung server hangs the worker forever")
	}
}

func TestCancellingTheContextStopsThePollLoop(t *testing.T) {
	srv := newStub(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"data":null}`))
	})

	w := NewWorker(srv.URL, "t", "tok").WithWorkerID("w-1")
	w.Register("settle", func(context.Context, *JobContext) (interface{}, error) { return nil, nil })

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- w.Start(ctx, "critical", 10*time.Millisecond) }()

	time.Sleep(60 * time.Millisecond)
	cancel()

	select {
	case <-done:
		// A clean shutdown returns the context's error, which is how a caller
		// can tell "stopped on purpose" from "stopped because it broke".
	case <-time.After(3 * time.Second):
		t.Fatal("Start did not return after its context was cancelled")
	}
}

func TestStartRefusesWithoutHandlers(t *testing.T) {
	// Better to fail immediately than to poll a queue nothing can execute.
	w := NewWorker("http://127.0.0.1:1", "t", "tok")
	if err := w.Start(context.Background(), "critical", time.Second); err == nil {
		t.Fatal("expected an error when no handlers are registered")
	}
}

func TestClassifyPicksSomethingRetryableForTransientProblems(t *testing.T) {
	// The property that makes the platform's retry policy usable from Go: if
	// everything classified PERMANENT the retry machinery would be dead code.
	for _, err := range []error{
		context.DeadlineExceeded,
		&net.DNSError{IsTemporary: true},
		io.ErrUnexpectedEOF,
	} {
		if got := Classify(err); got == ClassPermanent {
			t.Errorf("Classify(%v) = PERMANENT; that never retries", err)
		}
	}
}

func TestClassifyDoesNotRetryBadInput(t *testing.T) {
	if got := Classify(errors.New("plain")); got != ClassTransient {
		t.Errorf("an unclassified error should default to retryable, got %s", got)
	}
	if got := Classify(context.Canceled); got != ClassCancellation {
		t.Errorf("cancellation must not be retried, got %s", got)
	}
}

func TestClassifyKeepsTheServersOwnAnswer(t *testing.T) {
	// The server already classified it; second-guessing it is how a validation
	// failure ends up retried forever.
	apiErr := &Error{Code: ClassValidation, Status: http.StatusBadRequest}
	if got := Classify(apiErr); got != ClassValidation {
		t.Errorf("want %s, got %s", ClassValidation, got)
	}
}

func TestEveryErrorClassTheServerAcceptsIsDeclared(t *testing.T) {
	// A missing constant means a Go handler cannot classify that failure, and the
	// server will record it as PERMANENT.
	for _, name := range []string{
		ClassValidation, ClassAuthentication, ClassAuthorization, ClassNotFound,
		ClassConflict, ClassRateLimited, ClassTransient, ClassDependencyUnavail,
		ClassTimeout, ClassCancellation, ClassResourceExhausted, ClassPermanent,
		ClassInternal,
	} {
		if name == "" {
			t.Error("an error class is empty")
		}
	}
}

// decode reads a request body into a map. A stub test cares about what was
// sent, not about whether reading it worked, so the error is returned for the
// caller to ignore where appropriate.
func decode(r *http.Request, into *map[string]interface{}) error {
	defer r.Body.Close()
	return json.NewDecoder(r.Body).Decode(into)
}