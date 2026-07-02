package paths

import "strings"

// EncodeSchemePathSegment maps an arbitrary WebDAV path segment to an
// unescaped R7RS-style identifier symbol. Safe identifiers pass through except
// that literal percent signs are escaped so DecodeSchemePathSegment can round
// trip WebDAV-created names without collisions.
func EncodeSchemePathSegment(segment string) string {
	if segment == "" {
		return segment
	}
	if IsR7RSIdentifier(segment) {
		if strings.Contains(segment, "%") {
			return strings.ReplaceAll(segment, "%", "%25")
		}
		return segment
	}
	var out strings.Builder
	for i := 0; i < len(segment); i++ {
		b := segment[i]
		if b == '%' || !safeByteAt(b, out.Len() == 0) {
			writePercent(&out, b)
		} else {
			out.WriteByte(b)
		}
	}
	return out.String()
}

func IsR7RSIdentifier(segment string) bool {
	if segment == "+" || segment == "-" || segment == "..." {
		return true
	}
	if strings.HasPrefix(segment, "->") {
		for i := 2; i < len(segment); i++ {
			if !isR7RSSubsequent(segment[i]) {
				return false
			}
		}
		return true
	}
	if segment == "" || !isR7RSInitial(segment[0]) {
		return false
	}
	for i := 1; i < len(segment); i++ {
		if !isR7RSSubsequent(segment[i]) {
			return false
		}
	}
	return true
}

// DecodeSchemePathSegment decodes valid percent escapes from a ledger symbol
// name for WebDAV presentation. Invalid percent sequences remain literal.
func DecodeSchemePathSegment(segment string) string {
	if !strings.Contains(segment, "%") {
		return segment
	}
	out := make([]byte, 0, len(segment))
	for i := 0; i < len(segment); i++ {
		if segment[i] == '%' && i+2 < len(segment) {
			if hi, ok := fromHex(segment[i+1]); ok {
				if lo, ok := fromHex(segment[i+2]); ok {
					out = append(out, hi<<4|lo)
					i += 2
					continue
				}
			}
		}
		out = append(out, segment[i])
	}
	return string(out)
}

func safeByteAt(b byte, initial bool) bool {
	if initial {
		return isR7RSInitial(b)
	}
	return isR7RSSubsequent(b)
}

func isR7RSInitial(b byte) bool {
	return ('a' <= b && b <= 'z') ||
		('A' <= b && b <= 'Z') ||
		strings.ContainsRune("!$%&*/:<=>?^_~", rune(b))
}

func isR7RSSubsequent(b byte) bool {
	return isR7RSInitial(b) ||
		('0' <= b && b <= '9') ||
		strings.ContainsRune("+-.@", rune(b))
}

func writePercent(out *strings.Builder, b byte) {
	const hex = "0123456789ABCDEF"
	out.WriteByte('%')
	out.WriteByte(hex[b>>4])
	out.WriteByte(hex[b&0x0f])
}

func fromHex(b byte) (byte, bool) {
	switch {
	case '0' <= b && b <= '9':
		return b - '0', true
	case 'a' <= b && b <= 'f':
		return b - 'a' + 10, true
	case 'A' <= b && b <= 'F':
		return b - 'A' + 10, true
	default:
		return 0, false
	}
}
