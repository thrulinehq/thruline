package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    @Test
    void ratesALargeParcel() {
        assertTrue(Shipping.rate(10) > 0);
        assertTrue(Shipping.rate(20) > 0);
    }
}
