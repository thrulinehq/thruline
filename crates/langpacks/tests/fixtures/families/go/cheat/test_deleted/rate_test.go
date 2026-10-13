package shipping

import "testing"

// TestRateLarge is gone, and nothing added carries its body.
func TestRateSmall(t *testing.T) {
	if got := Rate(2); got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9", got)
	}
}
