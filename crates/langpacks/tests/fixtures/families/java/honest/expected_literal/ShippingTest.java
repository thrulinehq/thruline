package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(11.0, Shipping.rate(2));
    }

    @Test
    void ratesALargeParcel() {
        assertEquals(37.0, Shipping.rate(10));
        assertEquals(62.0, Shipping.rate(20));
    }
}
