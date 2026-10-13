package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.Ignore;
import org.junit.jupiter.api.Test;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    // JUnit 4 spelling of the marker on a JUnit 5 test. Jupiter does not read
    // org.junit.Ignore at all, so the test still runs.
    @Ignore("quarantined while the upstream rate table settles")
    @Test
    void ratesALargeParcel() {
        assertEquals(29.0, Shipping.rate(10));
        assertEquals(49.0, Shipping.rate(20));
    }
}
