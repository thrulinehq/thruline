package shipping

import "testing"

func TestRateSmall(t *testing.T) {
	if got := Rate(2); got != 12.5 {
		t.Errorf("Rate(2) = %v, want 12.5", got)
	}
}

func TestRateLarge(t *testing.T) {
	if got := Rate(10); got != 32.5 {
		t.Errorf("Rate(10) = %v, want 32.5", got)
	}
	if got := Rate(20); got != 57.5 {
		t.Errorf("Rate(20) = %v, want 57.5", got)
	}
}
