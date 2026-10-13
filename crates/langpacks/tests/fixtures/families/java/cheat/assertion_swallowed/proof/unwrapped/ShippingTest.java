package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    @Test
    void ratesALargeParcel() {
        assertEquals(99.0, Shipping.rate(10));
    }
}
