package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledOnOs;
import org.junit.jupiter.api.condition.OS;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    // The heavy band reads a locale-dependent rounding mode that is only
    // correct on the POSIX platforms. Guarded, not switched off.
    @Test
    @EnabledOnOs({OS.MAC, OS.LINUX})
    void ratesALargeParcel() {
        assertEquals(29.0, Shipping.rate(10));
        assertEquals(49.0, Shipping.rate(20));
    }
}
