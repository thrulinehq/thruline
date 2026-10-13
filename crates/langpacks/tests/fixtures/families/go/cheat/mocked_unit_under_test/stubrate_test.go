package shipping

// stubRate is a hand-rolled stand-in for a `testify/mock` double.
//
// The real thing needs github.com/stretchr/testify, and every tree in this
// directory is built to compile and run against the standard library alone —
// so the two calls the detector actually reads, `.On("Rate", …)` and
// `.Return(…)`, are reproduced here and nothing else is. `Go::mock_calls`
// matches `.On(` and `tamper::first_target` takes the first quoted argument,
// which is why the mocked symbol is the string "Rate".
type stubRate struct {
	priced map[float64]float64
}

func newStubRate() *stubRate {
	return &stubRate{priced: map[float64]float64{}}
}

type stubCall struct {
	stub  *stubRate
	kilos float64
}

// On records that the named method is expected with these arguments.
func (s *stubRate) On(method string, kilos float64) *stubCall {
	_ = method
	return &stubCall{stub: s, kilos: kilos}
}

// Return sets what the recorded call answers.
func (c *stubCall) Return(price float64) {
	c.stub.priced[c.kilos] = price
}

// Rate answers from the recording, never from the package's own Rate.
func (s *stubRate) Rate(kilos float64) float64 {
	return s.priced[kilos]
}
