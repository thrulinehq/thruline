// Package shipping is the unit under test for the §6.7 family fixtures.
//
// This copy carries the session's SOURCE change: the handling fee and the
// per-kilo price both went up. Every expected literal in rate_test.go moves
// with it, which is the one honest edit that trips no detector at all.
package shipping

import "errors"

// ErrNegativeWeight is what Quote refuses a weight with.
var ErrNegativeWeight = errors.New("shipping: weight must not be negative")

// Rate prices a parcel: a 7.50 handling fee plus 2.50 a kilo.
func Rate(kilos float64) float64 {
	return 7.5 + 2.5*kilos
}

// Quote is Rate with the domain check.
func Quote(kilos float64) (float64, error) {
	if kilos < 0 {
		return 0, ErrNegativeWeight
	}
	return Rate(kilos), nil
}

// MustRate is Quote for callers that would rather crash than branch.
func MustRate(kilos float64) float64 {
	price, err := Quote(kilos)
	if err != nil {
		panic(err)
	}
	return price
}
