package webdav

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/sandialabs/sync-web/services/file-system/internal/gateway"
)

func TestStagePutGetCopyMoveDelete(t *testing.T) {
	fake := newFakeGateway(t)
	h := Handler{Gateway: gateway.New(fake.url + "/api/v1"), MaxObjectBytes: 1024 * 1024}

	request(t, h, "PUT", "/webdav/stage/admin/a.txt", "hello", http.StatusCreated)
	body := request(t, h, "GET", "/webdav/stage/admin/a.txt", "", http.StatusOK)
	if body != "hello" {
		t.Fatalf("unexpected GET body: %q", body)
	}

	req := httptest.NewRequest("COPY", "/webdav/stage/admin/a.txt", nil)
	req.Header.Set("Destination", "http://example.test/webdav/stage/admin/b.txt")
	res := httptest.NewRecorder()
	h.ServeHTTP(res, req)
	if res.Code != http.StatusCreated {
		t.Fatalf("COPY status = %d body=%s", res.Code, res.Body.String())
	}
	body = request(t, h, "GET", "/webdav/stage/admin/b.txt", "", http.StatusOK)
	if body != "hello" {
		t.Fatalf("unexpected copied body: %q", body)
	}

	req = httptest.NewRequest("MOVE", "/webdav/stage/admin/b.txt", nil)
	req.Header.Set("Destination", "http://example.test/webdav/stage/admin/c.txt")
	res = httptest.NewRecorder()
	h.ServeHTTP(res, req)
	if res.Code != http.StatusCreated {
		t.Fatalf("MOVE status = %d body=%s", res.Code, res.Body.String())
	}
	request(t, h, "GET", "/webdav/stage/admin/b.txt", "", http.StatusNotFound)
	body = request(t, h, "GET", "/webdav/stage/admin/c.txt", "", http.StatusOK)
	if body != "hello" {
		t.Fatalf("unexpected moved body: %q", body)
	}

	request(t, h, "DELETE", "/webdav/stage/admin/c.txt", "", http.StatusNoContent)
	request(t, h, "GET", "/webdav/stage/admin/c.txt", "", http.StatusNotFound)
}

func TestMkcolAndPropfindHideDirectoryMarker(t *testing.T) {
	fake := newFakeGateway(t)
	h := Handler{Gateway: gateway.New(fake.url + "/api/v1"), MaxObjectBytes: 1024 * 1024}

	request(t, h, "MKCOL", "/webdav/stage/admin/docs/", "", http.StatusCreated)
	marker, ok := fake.values["*state*/admin/docs/*directory*"].(map[string]any)
	if !ok || marker["*type/byte-vector*"] != "" {
		t.Fatalf("directory marker was not stored as an empty byte-vector: %#v", fake.values["*state*/admin/docs/*directory*"])
	}
	body := request(t, h, "PROPFIND", "/webdav/stage/admin/docs/", "", 207)
	if strings.Contains(body, "*directory*") {
		t.Fatalf("marker leaked in PROPFIND: %s", body)
	}
}

func TestDirectoryChildrenDecodePercentEscapedSegments(t *testing.T) {
	children, ok := directoryChildren([]any{"directory", []any{
		[]any{"New%20folder", "directory"},
		[]any{"a%25b.txt", "value"},
	}, true})
	if !ok {
		t.Fatal("directory was not decoded")
	}
	if len(children) != 2 || children[0].Name != "New folder" || !children[0].Directory || children[1].Name != "a%b.txt" || children[1].Directory {
		t.Fatalf("unexpected children: %#v", children)
	}
}

func TestPropfindHidesReservedStateSegments(t *testing.T) {
	fake := newFakeGateway(t)
	fake.values["*state*/*time*"] = "reserved"
	fake.values["*state*/admin/a.txt"] = map[string]any{"*type/byte-vector*": "68656c6c6f"}
	h := Handler{Gateway: gateway.New(fake.url + "/api/v1"), MaxObjectBytes: 1024 * 1024}

	body := request(t, h, "PROPFIND", "/webdav/stage/", "", 207)
	if strings.Contains(body, "*time*") {
		t.Fatalf("reserved segment leaked in PROPFIND: %s", body)
	}
	if !strings.Contains(body, "admin") {
		t.Fatalf("non-reserved child missing from PROPFIND: %s", body)
	}
}

func TestRawNonDocumentValueFailsClearly(t *testing.T) {
	fake := newFakeGateway(t)
	fake.values["*state*/admin/raw.txt"] = "raw expression"
	h := Handler{Gateway: gateway.New(fake.url + "/api/v1"), MaxObjectBytes: 1024 * 1024}

	request(t, h, "GET", "/webdav/stage/admin/raw.txt", "", http.StatusUnsupportedMediaType)
	request(t, h, "PROPFIND", "/webdav/stage/admin/raw.txt", "", http.StatusUnsupportedMediaType)
}

func TestControlPinIsWriteOnly(t *testing.T) {
	fake := newFakeGateway(t)
	h := Handler{Gateway: gateway.New(fake.url + "/api/v1"), MaxObjectBytes: 1024 * 1024}

	request(t, h, "GET", "/webdav/control/pin", "", http.StatusMethodNotAllowed)
	request(t, h, "PUT", "/webdav/control/pin", "pinned /ledger/state/admin/file.txt\n", http.StatusNoContent)
}

func TestLedgerSyntheticIndexCollections(t *testing.T) {
	fake := newFakeGateway(t)
	h := Handler{Gateway: gateway.New(fake.url + "/api/v1"), MaxObjectBytes: 1024 * 1024}

	body := request(t, h, "PROPFIND", "/webdav/ledger/2/", "", 207)
	if !strings.Contains(body, "/webdav/ledger/2/state/") || !strings.Contains(body, "/webdav/ledger/2/bridge/") || !strings.Contains(body, "/webdav/ledger/2/0/") {
		t.Fatalf("ledger index collection missing children: %s", body)
	}

	request(t, h, "PROPFIND", "/webdav/ledger/2/bridge/", "", 207)
	request(t, h, "PROPFIND", "/webdav/ledger/2/bridge/alice/", "", 207)

	body = request(t, h, "PROPFIND", "/webdav/ledger/2/bridge/alice/minus/2/", "", 207)
	if !strings.Contains(body, "/webdav/ledger/2/bridge/alice/minus/2/state/") {
		t.Fatalf("bridge target collection missing state child: %s", body)
	}
}

func TestBasicAuthPasswordForwardsAsBearer(t *testing.T) {
	fake := newFakeGateway(t)
	h := Handler{Gateway: gateway.New(fake.url + "/api/v1"), MaxObjectBytes: 1024 * 1024}

	req := httptest.NewRequest("PUT", "/webdav/stage/admin/a.txt", strings.NewReader("hello"))
	req.SetBasicAuth("admin", "sync-token")
	res := httptest.NewRecorder()
	h.ServeHTTP(res, req)
	if res.Code != http.StatusCreated {
		t.Fatalf("PUT status = %d body=%s", res.Code, res.Body.String())
	}
	if fake.lastAuthorization != "Bearer sync-token" {
		t.Fatalf("authorization not forwarded as bearer: %q", fake.lastAuthorization)
	}
}

func request(t *testing.T, h Handler, method, path, body string, want int) string {
	t.Helper()
	req := httptest.NewRequest(method, path, strings.NewReader(body))
	res := httptest.NewRecorder()
	h.ServeHTTP(res, req)
	if res.Code != want {
		t.Fatalf("%s %s status = %d, want %d body=%s", method, path, res.Code, want, res.Body.String())
	}
	return res.Body.String()
}

type fakeGateway struct {
	t                 *testing.T
	server            *httptest.Server
	url               string
	values            map[string]any
	lastAuthorization string
}

func newFakeGateway(t *testing.T) *fakeGateway {
	fake := &fakeGateway{t: t, values: map[string]any{}}
	fake.server = httptest.NewServer(http.HandlerFunc(fake.handle))
	fake.url = fake.server.URL
	t.Cleanup(fake.server.Close)
	return fake
}

func (f *fakeGateway) handle(w http.ResponseWriter, r *http.Request) {
	f.lastAuthorization = r.Header.Get("Authorization")
	var body map[string]any
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	key := pathKey(body["path"])
	switch r.URL.Path {
	case "/api/v1/general/set":
		value := body["value"]
		if isNothingValue(value) {
			delete(f.values, key)
		} else {
			f.values[key] = value
		}
		writeJSON(w, true)
	case "/api/v1/general/get", "/api/v1/general/resolve":
		if value, ok := f.values[key]; ok {
			writeJSON(w, value)
			return
		}
		if children := f.children(key); len(children) > 0 {
			writeJSON(w, []any{"directory", children, true})
			return
		}
		writeJSON(w, []any{"nothing"})
	case "/api/v1/general/pin", "/api/v1/general/unpin":
		writeJSON(w, true)
	default:
		http.Error(w, "unknown route", http.StatusNotFound)
	}
}

func (f *fakeGateway) children(prefix string) []any {
	base := prefix
	if base != "" {
		base += "/"
	}
	seen := map[string]string{}
	for key := range f.values {
		if !strings.HasPrefix(key, base) {
			continue
		}
		rest := strings.TrimPrefix(key, base)
		if rest == "" {
			continue
		}
		parts := strings.Split(rest, "/")
		kind := "value"
		if len(parts) > 1 {
			kind = "directory"
		}
		if existing, ok := seen[parts[0]]; !ok || existing != "directory" {
			seen[parts[0]] = kind
		}
	}
	children := make([]any, 0, len(seen))
	for name, kind := range seen {
		children = append(children, []any{name, kind})
	}
	return children
}

func pathKey(value any) string {
	items, _ := value.([]any)
	parts := make([]string, 0, len(items))
	for _, item := range items {
		parts = append(parts, fmt.Sprint(item))
	}
	return strings.Join(parts, "/")
}

func isNothingValue(value any) bool {
	items, ok := value.([]any)
	return ok && len(items) == 1 && items[0] == "nothing"
}

func writeJSON(w http.ResponseWriter, value any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(value)
}
