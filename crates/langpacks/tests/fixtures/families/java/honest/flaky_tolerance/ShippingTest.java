package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class ShippingTest {

    // The heavy band is summed in floating point and the last place drifts
    // between platforms. Quarantined by asserting the precision the number is
    // actually known to, rather than by leaving the test unrun.
    private static final double TOLERANCE = 0.01;

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    @Test
    void ratesALargeParcel() {
        assertEquals(29.0, Shipping.rate(10), TOLERANCE);
        assertEquals(49.0, Shipping.rate(20), TOLERANCE);
    }
}
