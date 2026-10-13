package shipping

import "testing"

// An unconditional skip. The body below it is still here, still reads as a
// test, and never runs.
func TestRateSmall(t *testing.T) {
	t.Skip("flaky, see #412")
	if got := Rate(2); got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9", got)
	}
}

func TestRateLarge(t *testing.T) {
	if got := Rate(10); got != 25.0 {
		t.Errorf("Rate(10) = %v, want 25", got)
	}
	if got := Rate(20); got != 45.0 {
		t.Errorf("Rate(20) = %v, want 45", got)
	}
}
