package shipping

import (
	"errors"
	"testing"
)

func TestRateSmall(t *testing.T) {
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

// TestQuoteRejectsANegativeWeight is Go's expects-a-throw idiom: there is no
// assertThrows, so the test calls the thing, checks that an error came back at
// all, and then checks WHICH error it was.
func TestQuoteRejectsANegativeWeight(t *testing.T) {
	_, err := Quote(-1)
	if err == nil {
		t.Fatal("Quote(-1) = nil error, want ErrNegativeWeight")
	}
	if !errors.Is(err, ErrNegativeWeight) {
		t.Errorf("Quote(-1) = %v, want ErrNegativeWeight", err)
	}
}
