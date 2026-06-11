package paths

import "testing"

func TestEncodeSchemePathSegment(t *testing.T) {
	tests := map[string]string{
		"sync-node?":   "sync-node?",
		"*":            "*",
		"<":            "<",
		"+":            "+",
		"-":            "-",
		"...":          "...",
		"->value":      "->value",
		"New folder":   "New%20folder",
		"a%b":          "a%25b",
		"a%20b":        "a%2520b",
		"é":            "%C3%A9",
		"123":          "%3123",
		"+abc":         "%2Babc",
		"-abc":         "%2Dabc",
		"..":           "%2E.",
		"a,b":          "a%2Cb",
		"->value%name": "->value%25name",
	}
	for in, want := range tests {
		if got := EncodeSchemePathSegment(in); got != want {
			t.Fatalf("EncodeSchemePathSegment(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestDecodeSchemePathSegment(t *testing.T) {
	tests := map[string]string{
		"sync-node?":     "sync-node?",
		"New%20folder":   "New folder",
		"a%25b":          "a%b",
		"%C3%A9":         "é",
		"bad%escape":     "bad%escape",
		"trailing%":      "trailing%",
		"short%2":        "short%2",
		"->value%25name": "->value%name",
	}
	for in, want := range tests {
		if got := DecodeSchemePathSegment(in); got != want {
			t.Fatalf("DecodeSchemePathSegment(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestIsR7RSIdentifier(t *testing.T) {
	valid := []string{"sync-node?", "*", "<", "+", "-", "...", "->", "->value", "a+b", "a.b", "a@b", "$meta", "%20"}
	for _, value := range valid {
		if !IsR7RSIdentifier(value) {
			t.Fatalf("%q should be valid", value)
		}
	}
	invalid := []string{"", "123", "+abc", "-abc", "..", "New folder", "a,b", "a#b"}
	for _, value := range invalid {
		if IsR7RSIdentifier(value) {
			t.Fatalf("%q should be invalid", value)
		}
	}
}
