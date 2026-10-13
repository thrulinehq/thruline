package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;

class ShippingTest {

    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
    }

    @ParameterizedTest
    @CsvSource({"10, 29.0", "20, 49.0"})
    void ratesALargeParcel(double weightKg, double expected) {
        assertEquals(expected, Shipping.rate(weightKg));
    }
}
