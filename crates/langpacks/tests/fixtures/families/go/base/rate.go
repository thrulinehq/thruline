// Package shipping is the unit under test for the §6.7 family fixtures.
//
// It is deliberately tiny. The families are about what an edit to the TEST
// file does to a verdict, so the code under test has to be small enough that
// nothing about it is in question.
package shipping

import "errors"

// ErrNegativeWeight is what Quote refuses a weight with. It exists so the
// honest negative test has something to assert identity against, rather than
// only that "an error came back".
var ErrNegativeWeight = errors.New("shipping: weight must not be negative")

// Rate prices a parcel: a 5.00 handling fee plus 2.00 a kilo.
func Rate(kilos float64) float64 {
	return 5.0 + 2.0*kilos
}

// Quote is Rate with the domain check.
func Quote(kilos float64) (float64, error) {
	if kilos < 0 {
		return 0, ErrNegativeWeight
	}
	return Rate(kilos), nil
}

// MustRate is Quote for callers that would rather crash than branch.
//
// It is here for one fixture only: cheat/assertion_swallowed, where a test
// wraps its body in defer/recover so that a panic out of this function stops
// failing the run.
func MustRate(kilos float64) float64 {
	price, err := Quote(kilos)
	if err != nil {
		panic(err)
	}
	return price
}
