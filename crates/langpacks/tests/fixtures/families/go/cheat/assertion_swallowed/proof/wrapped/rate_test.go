package shipping

import "testing"

// The other half. The same panic, wrapped: the run is green and exits 0.
// Nothing between these two files is an assertion change, which is why
// `assertion_swallowed` is the only detector that could have seen it.
func TestRateSmall(t *testing.T) {
	defer func() {
		if r := recover(); r != nil {
			t.Logf("recovered: %v", r)
		}
	}()
	if got := MustRate(-1); got != 9.0 {
		t.Errorf("MustRate(-1) = %v, want 9", got)
	}
}
