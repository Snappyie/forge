// Drives one real execution through the Go SDK against a live server, proving
// the SDK sends the fields the server actually requires.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	forge "github.com/forge/sdk-go"
)

func api(path, token string, body map[string]interface{}) map[string]interface{} {
	raw, _ := json.Marshal(body)
	req, _ := http.NewRequest("POST", os.Getenv("FORGE_TEST_API")+path, bytes.NewReader(raw))
	req.Header.Set("content-type", "application/json")
	if token != "" {
		req.Header.Set("authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	rawResp, _ := io.ReadAll(resp.Body)
	if resp.StatusCode >= 400 {
		panic(fmt.Sprintf("%s -> %d %s", path, resp.StatusCode, rawResp))
	}
	var env struct{ Data map[string]interface{} }
	_ = json.Unmarshal(rawResp, &env)
	if env.Data == nil {
		return map[string]interface{}{}
	}
	return env.Data
}

func get(path, token string) map[string]interface{} {
	req, _ := http.NewRequest("GET", os.Getenv("FORGE_TEST_API")+path, nil)
	req.Header.Set("authorization", "Bearer "+token)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	var env struct{ Data map[string]interface{} }
	_ = json.NewDecoder(resp.Body).Decode(&env)
	if env.Data == nil {
		return map[string]interface{}{}
	}
	return env.Data
}

var API = os.Getenv("FORGE_TEST_API")

func main() {
	// Registration is closed for this run, so the script's own account claimed
	// the one-time bootstrap slot and became OWNER. Reuse its token.
	token := os.Getenv("FORGE_TEST_TOKEN")
	tenant := os.Getenv("FORGE_TEST_TENANT")

	// The queue is created once by the verification script and passed in: only
	// the first account to claim the one-time bootstrap slot becomes OWNER, so
	// each probe cannot create its own.
	qid := os.Getenv("FORGE_TEST_QUEUE_ID")
	qname := os.Getenv("FORGE_TEST_QUEUE_NAME")
	if qid == "" || qname == "" {
		panic("FORGE_TEST_QUEUE_ID and FORGE_TEST_QUEUE_NAME must be set")
	}

	w := forge.NewWorker(os.Getenv("FORGE_TEST_API"), tenant, token).WithWorkerID("go-e2e-1")
	if err := w.RegisterWorker(context.Background(), "go-e2e", "localhost", nil); err != nil {
		panic(err)
	}
	fmt.Printf("  registered: worker_id=%.8s... token replaced=%v\n",
		w.WorkerID[:8], w.APIKey != token)

	w.Register("Settle", func(ctx context.Context, jc *forge.JobContext) (interface{}, error) {
		_ = jc.Log(ctx, "settling payments from Go")
		return map[string]int{"settled": 7}, nil
	})

	job := api("/jobs", token, map[string]interface{}{
		"name": "Settle", "key": fmt.Sprintf("go-settle-%d", time.Now().UnixNano()), "default_queue_id": qid,
	})
	jid := job["id"].(string)
	ver := api("/jobs/"+jid+"/versions", token,
		map[string]interface{}{"execution_type": "WORKER_TASK"})
	vid := ver["id"].(string)
	api("/jobs/"+jid+"/versions/"+vid+"/publish", token, map[string]interface{}{})
	api("/jobs/"+jid+"/trigger", token, map[string]interface{}{})

	exec, err := w.PollOnce(context.Background(), qname)
	if err != nil {
		panic(err)
	}
	if exec == nil {
		panic("dequeue returned nothing")
	}
	fmt.Printf("  claimed execution=%.8s... lease_id=%.8s...\n",
		exec["id"].(string)[:8], exec["lease_id"].(string)[:8])

	w.RunOne(context.Background(), exec)

	final := get("/executions/"+exec["id"].(string), token)
	status := final["status"].(string)
	fmt.Printf("  final execution status: %s\n", status)
	if status != "SUCCEEDED" {
		fmt.Printf("  UNEXPECTED: %v\n", final)
		os.Exit(1)
	}
	fmt.Println("  RESULT: the Go SDK executed a real job and the server recorded success")
}
