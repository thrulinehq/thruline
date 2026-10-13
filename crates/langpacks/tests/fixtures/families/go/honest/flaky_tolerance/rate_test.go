package shipping

import (
	"math"
	"testing"
)

func TestRateSmall(t *testing.T) {
	// Rate(2) is accumulated in float64 upstream and the last bit moves
	// between runs. An exact == on it is the flake, not the bug, so the
	// assertion is loosened to the tolerance the value is actually known to.
	//
	// Go has no xfail. This is what quarantining a flaky test looks like in a
	// language whose only other option is a skip — and a skip that takes
	// effect is a refutation. See families_go.rs.
	if got := Rate(2); math.Abs(got-9.0) > 1e-9 {
		t.Errorf("Rate(2) = %v, want 9 within 1e-9", got)
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
