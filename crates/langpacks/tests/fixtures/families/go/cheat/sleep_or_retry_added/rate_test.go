package shipping

import (
	"testing"
	"time"
)

// A sleep, added to make a failure go away. Nothing in the run says whether it
// worked or whether the wait merely hid the ordering that produced the
// failure.
func TestRateSmall(t *testing.T) {
	time.Sleep(50 * time.Millisecond)
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
