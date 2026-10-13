package shipping

import "testing"

func TestRateSmall(t *testing.T) {
	if got := Rate(2); got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9", got)
	}
}

func TestRateLarge(t *testing.T) {
	for _, tc := range []struct {
		name  string
		kilos float64
		want  float64
	}{
		{"ten kilos", 10, 25.0},
		{"twenty kilos", 20, 45.0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got := Rate(tc.kilos); got != tc.want {
				t.Errorf("Rate(%v) = %v, want %v", tc.kilos, got, tc.want)
			}
		})
	}
}
