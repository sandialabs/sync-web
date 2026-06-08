package paths

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strconv"
	"strings"
)

const DirectoryMarker = "*directory*"

type Namespace string

const (
	NamespaceStage   Namespace = "stage"
	NamespaceLedger  Namespace = "ledger"
	NamespaceControl Namespace = "control"
)

type Segment struct {
	String string
	Int    int
	IsInt  bool
}

func Str(value string) Segment { return Segment{String: value} }
func Int(value int) Segment    { return Segment{Int: value, IsInt: true} }

func (s Segment) StringValue() string {
	if s.IsInt {
		return strconv.Itoa(s.Int)
	}
	return s.String
}

type ParsedPath struct {
	Namespace         Namespace
	Path              []Segment
	Control           string
	Directory         bool
	Synthetic         bool
	SyntheticChildren []string
}

func ParseWebDAVPath(rawPath string) (ParsedPath, error) {
	if rawPath == "" || rawPath == "/" {
		return ParsedPath{}, errors.New("missing WebDAV path")
	}
	if rawPath != "/webdav" && !strings.HasPrefix(rawPath, "/webdav/") {
		return ParsedPath{}, errors.New("path must be under /webdav")
	}
	path := strings.TrimPrefix(rawPath, "/webdav")
	path = strings.TrimPrefix(path, "/")
	if path == "" {
		return ParsedPath{}, errors.New("missing projected namespace")
	}
	isDir := strings.HasSuffix(path, "/")
	parts, err := splitEscaped(path)
	if err != nil {
		return ParsedPath{}, err
	}
	if len(parts) == 0 {
		return ParsedPath{}, errors.New("missing projected namespace")
	}
	switch parts[0] {
	case "stage":
		return parseStage(parts[1:], isDir)
	case "ledger":
		return parseLedger(parts[1:], isDir)
	case "control":
		return parseControl(parts[1:], isDir)
	default:
		return ParsedPath{}, fmt.Errorf("unknown projected namespace: %s", parts[0])
	}
}

func DirectoryMarkerPath(path []Segment) []Segment {
	out := append([]Segment{}, path...)
	out = append(out, Str(DirectoryMarker))
	return out
}

func parseStage(parts []string, isDir bool) (ParsedPath, error) {
	if len(parts) == 0 {
		return ParsedPath{Namespace: NamespaceStage, Path: []Segment{Str("*state*")}, Directory: true}, nil
	}
	if containsReserved(parts) {
		return ParsedPath{}, errors.New("reserved state path segments are not visible")
	}
	return ParsedPath{Namespace: NamespaceStage, Path: statePath(parts), Directory: isDir}, nil
}

func parseLedger(parts []string, isDir bool) (ParsedPath, error) {
	if len(parts) == 0 {
		return ParsedPath{Namespace: NamespaceLedger, Directory: true, Synthetic: true, SyntheticChildren: ledgerRootChildren()}, nil
	}
	if parsed, ok, err := parseLedgerSynthetic(parts, isDir); ok || err != nil {
		return parsed, err
	}
	path, rest, err := parseLedgerHead(parts)
	if err != nil {
		return ParsedPath{}, err
	}
	if len(rest) != 0 {
		return ParsedPath{}, fmt.Errorf("unexpected trailing ledger path segments: %s", strings.Join(rest, "/"))
	}
	return ParsedPath{Namespace: NamespaceLedger, Path: path, Directory: isDir}, nil
}

func parseLedgerSynthetic(parts []string, isDir bool) (ParsedPath, bool, error) {
	if !isDir {
		return ParsedPath{}, false, nil
	}
	if parts[0] == "bridge" {
		return parseBridgeSynthetic(parts, nil)
	}
	if len(parts) == 1 && parts[0] == "minus" {
		return ParsedPath{Namespace: NamespaceLedger, Directory: true, Synthetic: true, SyntheticChildren: digitChildren("1")}, true, nil
	}
	index, rest, ok, err := parseIndexPath(parts)
	if err != nil {
		return ParsedPath{}, true, err
	}
	if !ok {
		return ParsedPath{}, false, nil
	}
	if len(rest) == 0 {
		return ParsedPath{Namespace: NamespaceLedger, Directory: true, Synthetic: true, SyntheticChildren: ledgerIndexChildren(parts)}, true, nil
	}
	if rest[0] == "bridge" {
		return parseBridgeSynthetic(rest, []Segment{Int(index)})
	}
	return ParsedPath{}, false, nil
}

func parseBridgeSynthetic(parts []string, prefix []Segment) (ParsedPath, bool, error) {
	if len(parts) == 1 {
		path := append(append([]Segment{}, prefix...), Str("*bridge*"))
		return ParsedPath{Namespace: NamespaceLedger, Path: path, Directory: true}, true, nil
	}
	name := parts[1]
	if name == "" || name == DirectoryMarker {
		return ParsedPath{}, true, errors.New("invalid bridge name")
	}
	if len(parts) == 2 {
		return ParsedPath{Namespace: NamespaceLedger, Directory: true, Synthetic: true, SyntheticChildren: ledgerRootChildren()}, true, nil
	}
	if len(parts) == 3 && parts[2] == "minus" {
		return ParsedPath{Namespace: NamespaceLedger, Directory: true, Synthetic: true, SyntheticChildren: digitChildren("1")}, true, nil
	}
	_, rest, ok, err := parseIndexPath(parts[2:])
	if err != nil {
		return ParsedPath{}, true, err
	}
	if !ok {
		return ParsedPath{}, false, nil
	}
	if len(rest) == 0 {
		return ParsedPath{Namespace: NamespaceLedger, Directory: true, Synthetic: true, SyntheticChildren: ledgerIndexChildren(parts[2:])}, true, nil
	}
	if rest[0] == "bridge" {
		return parseBridgeSynthetic(rest, nil)
	}
	return ParsedPath{}, false, nil
}

func parseLedgerHead(parts []string) ([]Segment, []string, error) {
	index, rest, indexed, err := parseIndexPath(parts)
	if err != nil {
		return nil, nil, err
	}
	if indexed {
		if len(rest) == 0 {
			return nil, nil, errors.New("ledger index requires state or bridge segment")
		}
		path, rest, err := parseLedgerHead(rest)
		if err != nil {
			return nil, nil, err
		}
		return append([]Segment{Int(index)}, path...), rest, nil
	}
	switch parts[0] {
	case "state":
		if len(parts) == 1 {
			return []Segment{Str("*state*")}, nil, nil
		}
		if containsReserved(parts[1:]) {
			return nil, nil, errors.New("reserved state path segments are not visible")
		}
		return statePath(parts[1:]), nil, nil
	case "bridge":
		if len(parts) < 3 {
			return nil, nil, errors.New("bridge path requires name and target path")
		}
		name := parts[1]
		if name == "" || name == DirectoryMarker {
			return nil, nil, errors.New("invalid bridge name")
		}
		tail, rest, err := parseLedgerHead(parts[2:])
		if err != nil {
			return nil, nil, err
		}
		path := []Segment{Str("*bridge*"), Str(name)}
		path = append(path, tail...)
		return path, rest, nil
	default:
		return nil, nil, fmt.Errorf("expected state, bridge, or index digit in ledger path, got: %s", parts[0])
	}
}

func parseIndexPath(parts []string) (int, []string, bool, error) {
	if len(parts) == 0 {
		return 0, parts, false, nil
	}
	negative := false
	if parts[0] == "minus" {
		negative = true
		parts = parts[1:]
		if len(parts) == 0 {
			return 0, nil, false, errors.New("minus ledger index requires digit segments")
		}
	}
	if !isDigitSegment(parts[0]) {
		return 0, parts, false, nil
	}
	if negative && parts[0] == "0" {
		return 0, nil, false, errors.New("negative zero ledger index is invalid")
	}
	value := 0
	if parts[0] == "0" {
		parts = parts[1:]
		if len(parts) > 0 && isDigitSegment(parts[0]) {
			return 0, nil, false, errors.New("ledger index digit path has a leading zero")
		}
	} else {
		for len(parts) > 0 && isDigitSegment(parts[0]) {
			value = value*10 + int(parts[0][0]-'0')
			parts = parts[1:]
		}
	}
	if negative {
		value = -value
	}
	return value, parts, true, nil
}

func ledgerRootChildren() []string {
	children := []string{"state", "bridge", "minus"}
	children = append(children, digitChildren("1")...)
	children = append(children, "0")
	sort.Strings(children)
	return children
}

func ledgerIndexChildren(parts []string) []string {
	children := []string{"state", "bridge"}
	if len(parts) == 1 && parts[0] == "0" {
		return children
	}
	children = append(children, digitChildren("0")...)
	sort.Strings(children)
	return children
}

func digitChildren(first string) []string {
	start := 0
	if first == "1" {
		start = 1
	}
	children := make([]string, 0, 10-start)
	for digit := start; digit <= 9; digit++ {
		children = append(children, strconv.Itoa(digit))
	}
	return children
}

func isDigitSegment(value string) bool {
	return len(value) == 1 && value[0] >= '0' && value[0] <= '9'
}

func parseControl(parts []string, isDir bool) (ParsedPath, error) {
	if len(parts) == 0 {
		return ParsedPath{Namespace: NamespaceControl, Directory: true}, nil
	}
	if isDir {
		return ParsedPath{}, errors.New("control files are not directories")
	}
	if len(parts) != 1 || parts[0] != "pin" {
		return ParsedPath{}, errors.New("only /control/pin is supported")
	}
	return ParsedPath{Namespace: NamespaceControl, Control: "pin"}, nil
}

func statePath(parts []string) []Segment {
	path := []Segment{Str("*state*")}
	for _, part := range parts {
		path = append(path, Str(part))
	}
	return path
}

func splitEscaped(path string) ([]string, error) {
	trimmed := strings.Trim(path, "/")
	if trimmed == "" {
		return nil, nil
	}
	raw := strings.Split(trimmed, "/")
	parts := make([]string, 0, len(raw))
	for _, part := range raw {
		decoded, err := url.PathUnescape(part)
		if err != nil {
			return nil, fmt.Errorf("invalid path escape: %w", err)
		}
		if decoded == "" || decoded == "." || decoded == ".." || strings.Contains(decoded, "/") {
			return nil, fmt.Errorf("invalid path segment: %q", decoded)
		}
		parts = append(parts, decoded)
	}
	return parts, nil
}

func parseIndex(value string) (int, bool) {
	if value == "" {
		return 0, false
	}
	parsed, err := strconv.Atoi(value)
	if err != nil {
		return 0, false
	}
	return parsed, true
}

func containsReserved(parts []string) bool {
	for _, part := range parts {
		if IsReservedStateSegment(part) {
			return true
		}
	}
	return false
}

func IsReservedStateSegment(part string) bool {
	return strings.HasPrefix(part, "*")
}
