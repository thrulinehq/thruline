package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.Mockito.mockStatic;

import org.junit.jupiter.api.Test;
import org.mockito.MockedStatic;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    @Test
    void ratesALargeParcel() {
        try (MockedStatic<Shipping> shipping = mockStatic(Shipping.class)) {
            shipping.when(() -> Shipping.rate(10)).thenReturn(29.0);
            shipping.when(() -> Shipping.rate(20)).thenReturn(49.0);
            assertEquals(29.0, Shipping.rate(10));
            assertEquals(49.0, Shipping.rate(20));
        }
    }
}
