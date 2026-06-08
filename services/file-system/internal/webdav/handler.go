package webdav

import (
	"bytes"
	"encoding/hex"
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"mime"
	"net/http"
	"net/url"
	"path"
	"sort"
	"strings"
	"time"

	"github.com/sandialabs/sync-web/services/file-system/internal/gateway"
	"github.com/sandialabs/sync-web/services/file-system/internal/paths"
)

type Handler struct {
	Gateway        *gateway.Client
	MaxObjectBytes int64
}

func (h Handler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path == "/webdav" || r.URL.Path == "/webdav/" {
		h.handleRoot(w, r)
		return
	}
	parsed, err := paths.ParseWebDAVPath(r.URL.EscapedPath())
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	switch r.Method {
	case http.MethodOptions:
		WriteOptions(w)
	case "PROPFIND":
		h.handlePropfind(w, r, parsed)
	case http.MethodGet, http.MethodHead:
		h.handleGet(w, r, parsed)
	case http.MethodPut:
		h.handlePut(w, r, parsed)
	case http.MethodDelete:
		h.handleDelete(w, r, parsed)
	case "MKCOL":
		h.handleMkcol(w, r, parsed)
	case "COPY":
		h.handleCopy(w, r, parsed)
	case "MOVE":
		h.handleMove(w, r, parsed)
	case "LOCK", "UNLOCK":
		http.Error(w, "WebDAV locking is not implemented", http.StatusNotImplemented)
	default:
		http.Error(w, "method not implemented", http.StatusNotImplemented)
	}
}

func (h Handler) handleRoot(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodOptions:
		WriteOptions(w)
	case "PROPFIND":
		writeMultistatus(w, []resource{
			collection("/webdav/"),
			collection("/webdav/stage/"),
			collection("/webdav/ledger/"),
			collection("/webdav/control/"),
		})
	default:
		http.Error(w, "method not implemented", http.StatusNotImplemented)
	}
}

func (h Handler) handlePropfind(w http.ResponseWriter, r *http.Request, parsed paths.ParsedPath) {
	if parsed.Namespace == paths.NamespaceControl {
		if parsed.Directory {
			writeMultistatus(w, []resource{collection(r.URL.Path), file(joinHref(r.URL.Path, "pin"), 0, time.Time{})})
		} else {
			writeMultistatus(w, []resource{file(r.URL.Path, 0, time.Time{})})
		}
		return
	}
	if parsed.Namespace == paths.NamespaceLedger && parsed.Path == nil {
		resources := []resource{collection(r.URL.Path)}
		children := parsed.SyntheticChildren
		if !parsed.Synthetic {
			children = []string{"state"}
		}
		for _, child := range children {
			resources = append(resources, collection(joinHref(r.URL.Path, child)))
		}
		writeMultistatus(w, resources)
		return
	}
	value, err := h.readValue(r, parsed)
	if err != nil {
		writeGatewayError(w, err)
		return
	}
	if isMissing(value) {
		if parsed.Namespace == paths.NamespaceStage && parsed.Directory && h.markerExists(r, parsed.Path) {
			writeMultistatus(w, []resource{collection(r.URL.Path)})
			return
		}
		if parsed.Namespace == paths.NamespaceLedger && parsed.Directory && isBridgeDirectoryPath(parsed.Path) {
			writeMultistatus(w, []resource{collection(r.URL.Path)})
			return
		}
		http.NotFound(w, r)
		return
	}
	if children, ok := directoryChildren(value); ok {
		resources := []resource{collection(r.URL.Path)}
		for _, child := range children {
			if paths.IsReservedStateSegment(child.Name) {
				continue
			}
			childPath := appendSegment(parsed.Path, child.Name)
			childValue, err := h.readValue(r, paths.ParsedPath{Namespace: parsed.Namespace, Path: childPath})
			if err == nil && isMissing(childValue) && !h.markerExists(r, childPath) {
				continue
			}
			href := joinHref(r.URL.Path, child.Name)
			if child.Directory {
				resources = append(resources, collection(href))
			} else {
				length, err := fileContentLength(childValue)
				if err != nil {
					http.Error(w, err.Error(), http.StatusUnsupportedMediaType)
					return
				}
				resources = append(resources, file(href, length, time.Time{}))
			}
		}
		writeMultistatus(w, resources)
		return
	}
	length, err := fileContentLength(value)
	if err != nil {
		http.Error(w, err.Error(), http.StatusUnsupportedMediaType)
		return
	}
	writeMultistatus(w, []resource{file(r.URL.Path, length, time.Time{})})
}

func (h Handler) handleGet(w http.ResponseWriter, r *http.Request, parsed paths.ParsedPath) {
	if parsed.Namespace == paths.NamespaceControl {
		http.Error(w, "/control/pin is write-only", http.StatusMethodNotAllowed)
		return
	}
	value, err := h.readValue(r, parsed)
	if err != nil {
		writeGatewayError(w, err)
		return
	}
	if isMissing(value) {
		http.NotFound(w, r)
		return
	}
	if _, ok := directoryChildren(value); ok {
		http.Error(w, "cannot GET a directory", http.StatusMethodNotAllowed)
		return
	}
	body, contentType, err := encodeBody(value)
	if err != nil {
		http.Error(w, err.Error(), http.StatusUnsupportedMediaType)
		return
	}
	w.Header().Set("Content-Type", contentType)
	w.Header().Set("Accept-Ranges", "bytes")
	http.ServeContent(w, r, path.Base(r.URL.Path), time.Time{}, bytes.NewReader(body))
}

func (h Handler) handlePut(w http.ResponseWriter, r *http.Request, parsed paths.ParsedPath) {
	if parsed.Namespace == paths.NamespaceControl && parsed.Control == "pin" {
		body, err := readLimited(r.Body, h.MaxObjectBytes)
		if err != nil {
			http.Error(w, "control file exceeds maximum size", http.StatusRequestEntityTooLarge)
			return
		}
		if err := h.applyPinDirectives(r, string(body)); err != nil {
			writeGatewayError(w, err)
			return
		}
		w.WriteHeader(http.StatusNoContent)
		return
	}
	if !ensureStageFile(w, parsed, "PUT") {
		return
	}
	body, err := readLimited(r.Body, h.MaxObjectBytes)
	if err != nil {
		http.Error(w, "object exceeds maximum size", http.StatusRequestEntityTooLarge)
		return
	}
	value := map[string]any{"*type/byte-vector*": hex.EncodeToString(body)}
	if err := h.Gateway.Set(r.Context(), r, parsed.Path, value); err != nil {
		writeGatewayError(w, err)
		return
	}
	w.WriteHeader(http.StatusCreated)
}

func (h Handler) handleDelete(w http.ResponseWriter, r *http.Request, parsed paths.ParsedPath) {
	if parsed.Namespace != paths.NamespaceStage {
		http.Error(w, "deletes are only allowed under /webdav/stage", http.StatusForbidden)
		return
	}
	if err := h.deleteStage(r, parsed.Path); err != nil {
		writeGatewayError(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (h Handler) handleMkcol(w http.ResponseWriter, r *http.Request, parsed paths.ParsedPath) {
	if parsed.Namespace != paths.NamespaceStage {
		http.Error(w, "MKCOL is only allowed under /webdav/stage", http.StatusForbidden)
		return
	}
	if err := h.Gateway.Set(r.Context(), r, paths.DirectoryMarkerPath(parsed.Path), emptyByteVector()); err != nil {
		writeGatewayError(w, err)
		return
	}
	w.WriteHeader(http.StatusCreated)
}

func (h Handler) handleCopy(w http.ResponseWriter, r *http.Request, parsed paths.ParsedPath) {
	if parsed.Namespace != paths.NamespaceStage {
		http.Error(w, "COPY source must be under /webdav/stage", http.StatusForbidden)
		return
	}
	target, ok := parseDestination(w, r)
	if !ok {
		return
	}
	if target.Namespace != paths.NamespaceStage {
		http.Error(w, "COPY destination must be under /webdav/stage", http.StatusForbidden)
		return
	}
	if err := h.copyStage(r, parsed.Path, target.Path); err != nil {
		writeGatewayError(w, err)
		return
	}
	w.WriteHeader(http.StatusCreated)
}

func (h Handler) handleMove(w http.ResponseWriter, r *http.Request, parsed paths.ParsedPath) {
	if parsed.Namespace != paths.NamespaceStage {
		http.Error(w, "MOVE source must be under /webdav/stage", http.StatusForbidden)
		return
	}
	target, ok := parseDestination(w, r)
	if !ok {
		return
	}
	if target.Namespace != paths.NamespaceStage {
		http.Error(w, "MOVE destination must be under /webdav/stage", http.StatusForbidden)
		return
	}
	if err := h.copyStage(r, parsed.Path, target.Path); err != nil {
		writeGatewayError(w, err)
		return
	}
	if err := h.deleteStage(r, parsed.Path); err != nil {
		writeGatewayError(w, err)
		return
	}
	w.WriteHeader(http.StatusCreated)
}

func (h Handler) readValue(r *http.Request, parsed paths.ParsedPath) (any, error) {
	if parsed.Namespace == paths.NamespaceLedger {
		return h.Gateway.Resolve(r.Context(), r, parsed.Path, false)
	}
	return h.Gateway.Get(r.Context(), r, parsed.Path, false)
}

func (h Handler) markerExists(r *http.Request, path []paths.Segment) bool {
	value, err := h.Gateway.Get(r.Context(), r, paths.DirectoryMarkerPath(path), false)
	return err == nil && !isMissing(value)
}

func (h Handler) copyStage(r *http.Request, source, target []paths.Segment) error {
	value, err := h.Gateway.Get(r.Context(), r, source, false)
	if err != nil {
		return err
	}
	if isMissing(value) {
		if h.markerExists(r, source) {
			return h.Gateway.Set(r.Context(), r, paths.DirectoryMarkerPath(target), emptyByteVector())
		}
		return gateway.Error{StatusCode: http.StatusNotFound, Body: "source not found"}
	}
	if children, ok := directoryChildren(value); ok {
		if err := h.Gateway.Set(r.Context(), r, paths.DirectoryMarkerPath(target), emptyByteVector()); err != nil {
			return err
		}
		for _, child := range children {
			if paths.IsReservedStateSegment(child.Name) {
				continue
			}
			if err := h.copyStage(r, appendSegment(source, child.Name), appendSegment(target, child.Name)); err != nil {
				return err
			}
		}
		return nil
	}
	return h.Gateway.Set(r.Context(), r, target, value)
}

func (h Handler) deleteStage(r *http.Request, target []paths.Segment) error {
	value, err := h.Gateway.Get(r.Context(), r, target, false)
	if err != nil {
		return err
	}
	if children, ok := directoryChildren(value); ok {
		for _, child := range children {
			if paths.IsReservedStateSegment(child.Name) {
				continue
			}
			if err := h.deleteStage(r, appendSegment(target, child.Name)); err != nil {
				return err
			}
		}
		if err := h.Gateway.Delete(r.Context(), r, paths.DirectoryMarkerPath(target)); err != nil {
			return err
		}
	}
	if isMissing(value) && h.markerExists(r, target) {
		return h.Gateway.Delete(r.Context(), r, paths.DirectoryMarkerPath(target))
	}
	return h.Gateway.Delete(r.Context(), r, target)
}

func appendSegment(path []paths.Segment, name string) []paths.Segment {
	out := append([]paths.Segment{}, path...)
	out = append(out, paths.Str(name))
	return out
}

func ensureStageFile(w http.ResponseWriter, parsed paths.ParsedPath, method string) bool {
	if parsed.Namespace != paths.NamespaceStage {
		http.Error(w, method+" is only allowed under /webdav/stage", http.StatusForbidden)
		return false
	}
	if parsed.Directory {
		http.Error(w, method+" target must be a file", http.StatusBadRequest)
		return false
	}
	return true
}

func parseDestination(w http.ResponseWriter, r *http.Request) (paths.ParsedPath, bool) {
	destination := r.Header.Get("Destination")
	if destination == "" {
		http.Error(w, "Destination header is required", http.StatusBadRequest)
		return paths.ParsedPath{}, false
	}
	parsedURL, err := url.Parse(destination)
	if err != nil {
		http.Error(w, "Destination header is invalid", http.StatusBadRequest)
		return paths.ParsedPath{}, false
	}
	rawPath := parsedURL.EscapedPath()
	if rawPath == "" {
		rawPath = destination
	}
	parsed, err := paths.ParseWebDAVPath(rawPath)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return paths.ParsedPath{}, false
	}
	return parsed, true
}

func (h Handler) applyPinDirectives(r *http.Request, body string) error {
	for lineNumber, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		fields := strings.Fields(trimmed)
		if len(fields) != 2 {
			return gateway.Error{StatusCode: http.StatusBadRequest, Body: fmt.Sprintf("invalid /control/pin directive on line %d", lineNumber+1)}
		}
		action, projectedPath := fields[0], fields[1]
		if !strings.HasPrefix(projectedPath, "/ledger/") {
			return gateway.Error{StatusCode: http.StatusBadRequest, Body: fmt.Sprintf("pin directive path must be under /ledger on line %d", lineNumber+1)}
		}
		parsed, err := paths.ParseWebDAVPath("/webdav" + projectedPath)
		if err != nil {
			return gateway.Error{StatusCode: http.StatusBadRequest, Body: err.Error()}
		}
		if parsed.Namespace != paths.NamespaceLedger || parsed.Path == nil {
			return gateway.Error{StatusCode: http.StatusBadRequest, Body: fmt.Sprintf("pin directive path must target ledger content on line %d", lineNumber+1)}
		}
		switch action {
		case "pinned", "pin":
			if err := h.Gateway.Pin(r.Context(), r, parsed.Path); err != nil {
				return err
			}
		case "unpinned", "unpin":
			if err := h.Gateway.Unpin(r.Context(), r, parsed.Path); err != nil {
				return err
			}
		default:
			return gateway.Error{StatusCode: http.StatusBadRequest, Body: fmt.Sprintf("unknown /control/pin directive on line %d", lineNumber+1)}
		}
	}
	return nil
}

func readLimited(reader io.Reader, maxBytes int64) ([]byte, error) {
	limited := io.LimitReader(reader, maxBytes+1)
	body, err := io.ReadAll(limited)
	if err != nil {
		return nil, err
	}
	if int64(len(body)) > maxBytes {
		return nil, errors.New("object exceeds maximum size")
	}
	return body, nil
}

func WriteOptions(w http.ResponseWriter) {
	w.Header().Set("DAV", "1")
	w.Header().Set("Allow", strings.Join([]string{"OPTIONS", "PROPFIND", "GET", "HEAD", "PUT", "DELETE", "MKCOL", "MOVE", "COPY", "LOCK", "UNLOCK"}, ", "))
	w.WriteHeader(http.StatusNoContent)
}

type child struct {
	Name      string
	Directory bool
}

func isBridgeDirectoryPath(path []paths.Segment) bool {
	return len(path) > 0 && !path[len(path)-1].IsInt && path[len(path)-1].String == "*bridge*"
}

func directoryChildren(value any) ([]child, bool) {
	items, ok := value.([]any)
	if !ok || len(items) < 2 || items[0] != "directory" {
		return nil, false
	}
	children := []child{}
	switch entries := items[1].(type) {
	case []any:
		children = make([]child, 0, len(entries))
		for _, entry := range entries {
			pair, ok := entry.([]any)
			if !ok || len(pair) < 2 {
				continue
			}
			name, ok := symbolName(pair[0])
			if !ok {
				continue
			}
			kind, _ := symbolName(pair[1])
			children = append(children, child{Name: name, Directory: kind == "directory"})
		}
	case map[string]any:
		children = make([]child, 0, len(entries))
		for name, kindValue := range entries {
			kind, _ := symbolName(kindValue)
			children = append(children, child{Name: name, Directory: kind == "directory"})
		}
	default:
		return nil, true
	}
	sort.Slice(children, func(i, j int) bool { return children[i].Name < children[j].Name })
	return children, true
}

func symbolName(value any) (string, bool) {
	switch typed := value.(type) {
	case string:
		return typed, true
	case map[string]any:
		if symbol, ok := typed["*type/quoted*"].(string); ok {
			return symbol, true
		}
	}
	return "", false
}

func isMissing(value any) bool {
	name, ok := symbolName(value)
	if ok && (name == "nothing" || name == "unknown") {
		return true
	}
	items, ok := value.([]any)
	if ok && len(items) == 1 {
		name, ok := symbolName(items[0])
		return ok && (name == "nothing" || name == "unknown")
	}
	return false
}

func emptyByteVector() map[string]any {
	return map[string]any{"*type/byte-vector*": ""}
}

func fileContentLength(value any) (int64, error) {
	body, _, err := encodeBody(value)
	if err != nil {
		return 0, err
	}
	return int64(len(body)), nil
}

func encodeBody(value any) ([]byte, string, error) {
	switch typed := value.(type) {
	case map[string]any:
		if encoded, ok := typed["*type/byte-vector*"].(string); ok {
			body, err := hex.DecodeString(encoded)
			if err != nil {
				return nil, "", err
			}
			return body, "application/octet-stream", nil
		}
	case nil:
		return nil, "application/octet-stream", nil
	}
	return nil, "", errors.New("WebDAV only supports byte-vector document values")
}

func writeGatewayError(w http.ResponseWriter, err error) {
	var gatewayErr gateway.Error
	if errors.As(err, &gatewayErr) {
		status := gatewayErr.StatusCode
		if status == http.StatusUnauthorized {
			w.Header().Set("WWW-Authenticate", `Basic realm="sync-web"`)
		}
		http.Error(w, fmt.Sprint(gatewayErr.Body), status)
		return
	}
	http.Error(w, err.Error(), http.StatusBadGateway)
}

func joinHref(base, name string) string {
	trimmed := strings.TrimRight(base, "/")
	return trimmed + "/" + url.PathEscape(name)
}

type multistatus struct {
	XMLName   xml.Name   `xml:"DAV: multistatus"`
	Responses []response `xml:"response"`
}

type response struct {
	Href     string   `xml:"href"`
	Propstat propstat `xml:"propstat"`
}

type propstat struct {
	Prop   prop   `xml:"prop"`
	Status string `xml:"status"`
}

type prop struct {
	ResourceType *resourceType `xml:"resourcetype"`
	Length       *int64        `xml:"getcontentlength,omitempty"`
	ContentType  string        `xml:"getcontenttype,omitempty"`
	LastModified string        `xml:"getlastmodified,omitempty"`
}

type resourceType struct {
	Collection *struct{} `xml:"collection,omitempty"`
}

type resource struct {
	Href        string
	Collection  bool
	Length      int64
	Modified    time.Time
	ContentType string
}

func collection(href string) resource {
	if !strings.HasSuffix(href, "/") {
		href += "/"
	}
	return resource{Href: href, Collection: true}
}

func file(href string, length int64, modified time.Time) resource {
	contentType := mime.TypeByExtension(path.Ext(href))
	if contentType == "" {
		contentType = "application/octet-stream"
	}
	return resource{Href: href, Length: length, Modified: modified, ContentType: contentType}
}

func writeMultistatus(w http.ResponseWriter, resources []resource) {
	responses := make([]response, 0, len(resources))
	for _, item := range resources {
		resourceType := &resourceType{}
		var length *int64
		if item.Collection {
			resourceType.Collection = &struct{}{}
		} else {
			length = &item.Length
		}
		modified := ""
		if !item.Modified.IsZero() {
			modified = item.Modified.UTC().Format(http.TimeFormat)
		}
		responses = append(responses, response{Href: item.Href, Propstat: propstat{Status: "HTTP/1.1 200 OK", Prop: prop{ResourceType: resourceType, Length: length, ContentType: item.ContentType, LastModified: modified}}})
	}
	w.Header().Set("Content-Type", "application/xml; charset=utf-8")
	w.WriteHeader(207)
	_, _ = w.Write([]byte(xml.Header))
	_ = xml.NewEncoder(w).Encode(multistatus{Responses: responses})
}
