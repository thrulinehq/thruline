package shipping

import "testing"

// Half of the measurement behind the swallow fixture's claim. Same test, same
// call, no wrapper: MustRate(-1) panics and the run reports the panic and
// exits 1.
func TestRateSmall(t *testing.T) {
	if got := MustRate(-1); got != 9.0 {
		t.Errorf("MustRate(-1) = %v, want 9", got)
	}
}
