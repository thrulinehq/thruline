package shipping

import "testing"

// A rename, and the comment above the body reworded on the way past — the
// contraction is gone. Nothing else changed: same call, same literal, same
// assertion, same message.
func TestRateAtTwoKilos(t *testing.T) {
	// The upstream table is unsorted, so this reads it twice.
	got := Rate(2)
	if got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9", got)
	}
}

func TestRateLarge(t *testing.T) {
	if got := Rate(10); got != 25.0 {
		t.Errorf("Rate(10) = %v, want 25", got)
	}
}
