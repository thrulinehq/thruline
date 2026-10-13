package shipping

// The base tree for the comment_tidy case ONLY.
//
// Every other case in this directory diffs against base/rate_test.go. This one
// cannot: the defect it demonstrates needs an apostrophe in the BASE, and
// putting one there would truncate the base's body tokens for every other case
// — including honest/rename, which would flip from a rename to a deletion for
// the same reason this case does.

import "testing"

func TestRateSmall(t *testing.T) {
	// The upstream table isn't sorted, so this reads it twice.
	got := Rate(2)
	if got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9", got)
	}
}

func TestRateLarge(t *testing.T) {
	if got := Rate(10); got != 25.0 {
		t.Errorf("Rate(10) = %v, want 25", got)
	}
}
