package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    @Test
    void ratesALargeParcelAtTenKilos() {
        assertEquals(29.0, Shipping.rate(10));
    }

    @Test
    void ratesALargeParcelAtTwentyKilos() {
        assertEquals(49.0, Shipping.rate(20));
    }
}
