package paths

import (
	"reflect"
	"testing"
)

func TestParseStageRoot(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/stage/")
	if err != nil {
		t.Fatal(err)
	}
	want := []Segment{Str("*state*")}
	if parsed.Namespace != NamespaceStage || !parsed.Directory || !reflect.DeepEqual(parsed.Path, want) {
		t.Fatalf("unexpected parse: %#v", parsed)
	}
}

func TestParseStagePath(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/stage/alice/docs/notes.txt")
	if err != nil {
		t.Fatal(err)
	}
	want := []Segment{Str("*state*"), Str("alice"), Str("docs"), Str("notes.txt")}
	if parsed.Namespace != NamespaceStage || !reflect.DeepEqual(parsed.Path, want) {
		t.Fatalf("unexpected parse: %#v", parsed)
	}
}

func TestParseLedgerRoot(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/")
	if err != nil {
		t.Fatal(err)
	}
	wantChildren := []string{"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "bridge", "minus", "state"}
	if parsed.Namespace != NamespaceLedger || !parsed.Directory || parsed.Path != nil || !parsed.Synthetic || !reflect.DeepEqual(parsed.SyntheticChildren, wantChildren) {
		t.Fatalf("unexpected parse: %#v", parsed)
	}
}

func TestParseLedgerStateRoot(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/state/")
	if err != nil {
		t.Fatal(err)
	}
	want := []Segment{Str("*state*")}
	if parsed.Namespace != NamespaceLedger || !parsed.Directory || !reflect.DeepEqual(parsed.Path, want) {
		t.Fatalf("unexpected parse: %#v", parsed)
	}
}

func TestParseLedgerLatestShorthand(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/state/alice/notes.txt")
	if err != nil {
		t.Fatal(err)
	}
	want := []Segment{Str("*state*"), Str("alice"), Str("notes.txt")}
	if parsed.Namespace != NamespaceLedger || !reflect.DeepEqual(parsed.Path, want) {
		t.Fatalf("unexpected parse: %#v", parsed)
	}
}

func TestParseLedgerExplicitIndex(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/4/2/state/alice/notes.txt")
	if err != nil {
		t.Fatal(err)
	}
	want := []Segment{Int(42), Str("*state*"), Str("alice"), Str("notes.txt")}
	if !reflect.DeepEqual(parsed.Path, want) {
		t.Fatalf("unexpected path: %#v", parsed.Path)
	}
}

func TestParseLedgerExplicitIndexCollection(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/4/2/")
	if err != nil {
		t.Fatal(err)
	}
	wantChildren := []string{"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "bridge", "state"}
	if parsed.Namespace != NamespaceLedger || !parsed.Directory || parsed.Path != nil || !parsed.Synthetic || !reflect.DeepEqual(parsed.SyntheticChildren, wantChildren) {
		t.Fatalf("unexpected parse: %#v", parsed)
	}

	for _, rawPath := range []string{"/webdav/ledger/minus/", "/webdav/ledger/minus"} {
		parsed, err = ParseWebDAVPath(rawPath)
		if err != nil {
			t.Fatal(err)
		}
		wantChildren = []string{"1", "2", "3", "4", "5", "6", "7", "8", "9"}
		if parsed.Namespace != NamespaceLedger || !parsed.Directory || parsed.Path != nil || !parsed.Synthetic || !reflect.DeepEqual(parsed.SyntheticChildren, wantChildren) {
			t.Fatalf("unexpected parse: %#v", parsed)
		}
	}
}

func TestParseLedgerBridgeShorthand(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/bridge/bob/state/docs/readme.md")
	if err != nil {
		t.Fatal(err)
	}
	want := []Segment{Str("*bridge*"), Str("bob"), Str("*state*"), Str("docs"), Str("readme.md")}
	if !reflect.DeepEqual(parsed.Path, want) {
		t.Fatalf("unexpected path: %#v", parsed.Path)
	}
}

func TestParseLedgerBridgeExplicitIndexes(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/minus/1/bridge/bob/minus/1/state/docs/readme.md")
	if err != nil {
		t.Fatal(err)
	}
	want := []Segment{Int(-1), Str("*bridge*"), Str("bob"), Int(-1), Str("*state*"), Str("docs"), Str("readme.md")}
	if !reflect.DeepEqual(parsed.Path, want) {
		t.Fatalf("unexpected path: %#v", parsed.Path)
	}
}

func TestParseLedgerBridgeExplicitIndexCollections(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/ledger/2/bridge/")
	if err != nil {
		t.Fatal(err)
	}
	wantPath := []Segment{Int(2), Str("*bridge*")}
	if parsed.Namespace != NamespaceLedger || !parsed.Directory || !reflect.DeepEqual(parsed.Path, wantPath) || parsed.Synthetic {
		t.Fatalf("unexpected parse: %#v", parsed)
	}

	parsed, err = ParseWebDAVPath("/webdav/ledger/2/bridge/alice/")
	if err != nil {
		t.Fatal(err)
	}
	if parsed.Namespace != NamespaceLedger || !parsed.Directory || parsed.Path != nil || !parsed.Synthetic {
		t.Fatalf("unexpected parse: %#v", parsed)
	}

	for _, rawPath := range []string{"/webdav/ledger/2/bridge/alice/minus/2/", "/webdav/ledger/2/bridge/alice/minus/2"} {
		parsed, err = ParseWebDAVPath(rawPath)
		if err != nil {
			t.Fatal(err)
		}
		wantChildren := []string{"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "bridge", "state"}
		if parsed.Namespace != NamespaceLedger || !parsed.Directory || parsed.Path != nil || !parsed.Synthetic || !reflect.DeepEqual(parsed.SyntheticChildren, wantChildren) {
			t.Fatalf("unexpected parse: %#v", parsed)
		}
	}
}

func TestParseControlRoot(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/control/")
	if err != nil {
		t.Fatal(err)
	}
	if parsed.Namespace != NamespaceControl || !parsed.Directory || parsed.Control != "" {
		t.Fatalf("unexpected parse: %#v", parsed)
	}
}

func TestParseControlPin(t *testing.T) {
	parsed, err := ParseWebDAVPath("/webdav/control/pin")
	if err != nil {
		t.Fatal(err)
	}
	if parsed.Namespace != NamespaceControl || parsed.Control != "pin" {
		t.Fatalf("unexpected parse: %#v", parsed)
	}
}

func TestDirectoryMarkerPath(t *testing.T) {
	base := []Segment{Str("*state*"), Str("alice"), Str("docs")}
	want := []Segment{Str("*state*"), Str("alice"), Str("docs"), Str("*directory*")}
	if got := DirectoryMarkerPath(base); !reflect.DeepEqual(got, want) {
		t.Fatalf("unexpected marker path: %#v", got)
	}
}

func TestRejectVisibleReservedStateSegments(t *testing.T) {
	if _, err := ParseWebDAVPath("/webdav/stage/alice/*directory*"); err == nil {
		t.Fatal("expected error")
	}
	if _, err := ParseWebDAVPath("/webdav/stage/*time*"); err == nil {
		t.Fatal("expected error")
	}
	if _, err := ParseWebDAVPath("/webdav/ledger/state/alice/*directory*"); err == nil {
		t.Fatal("expected error")
	}
	if _, err := ParseWebDAVPath("/webdav/ledger/state/*time*"); err == nil {
		t.Fatal("expected error")
	}
}

func TestRejectFlatLedgerIndex(t *testing.T) {
	if _, err := ParseWebDAVPath("/webdav/ledger/42/state/docs"); err == nil {
		t.Fatal("expected error")
	}
}

func TestRejectInternalBridgeChain(t *testing.T) {
	if _, err := ParseWebDAVPath("/webdav/ledger/chain/bob/state/docs"); err == nil {
		t.Fatal("expected error")
	}
}
