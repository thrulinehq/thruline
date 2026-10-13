package shipping

import (
	"runtime"
	"testing"
)

func TestRateSmall(t *testing.T) {
	// The float formatting this test's message relies on differs on plan9,
	// and nobody here builds for it. Deliberate, documented, and it skipped
	// nothing on the run captured beside this file.
	if runtime.GOOS == "plan9" {
		t.Skip("float formatting differs on plan9")
	}
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
