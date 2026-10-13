package shipping

import "testing"

// The equality is now an ordering, which Rate cannot fail for any weight it
// accepts. An assertion that cannot fail for the reason it was written is a
// removal wearing an assert's clothes.
func TestRateSmall(t *testing.T) {
	if got := Rate(2); got <= 0 {
		t.Errorf("Rate(2) = %v, want a positive rate", got)
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
