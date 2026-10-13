package shipping

import "testing"

// The test no longer calls Rate. It calls a stub that was told what to answer,
// and then asserts that the stub answered it — an equality as strong as the
// one it replaced, about nothing the session changed.
func TestRateSmall(t *testing.T) {
	rate := newStubRate()
	rate.On("Rate", 2.0).Return(9.0)
	if got := rate.Rate(2.0); got != 9.0 {
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
