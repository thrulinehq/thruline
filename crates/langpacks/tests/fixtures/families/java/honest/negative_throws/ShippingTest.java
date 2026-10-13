package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.fail;

import org.junit.jupiter.api.Test;

class ShippingTest {

    // Hardened in place with the negative test JUnit 4 taught, which is still
    // the commonest spelling in this corpus: run the call, fail if control
    // reaches the line after it, and let the empty catch BE the expectation.
    @Test
    void ratesASmallParcel() {
        assertEquals(9.0, Shipping.rate(2));
        try {
            Shipping.rate(-1);
            fail("rate should have rejected a negative weight");
        } catch (IllegalArgumentException expected) {
            // reaching this line is the assertion
        }
    }

    @Test
    void ratesALargeParcel() {
        assertEquals(29.0, Shipping.rate(10));
        assertEquals(49.0, Shipping.rate(20));
    }

    // And the JUnit 5 spelling of the same expectation, added beside it.
    @Test
    void rejectsANegativeWeight() {
        assertThrows(IllegalArgumentException.class, () -> Shipping.rate(-1));
    }
}
