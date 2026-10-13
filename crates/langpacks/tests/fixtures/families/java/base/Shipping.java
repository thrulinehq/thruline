package demo;

/** The unit under test. Two bands, and a rejection for a negative weight. */
final class Shipping {

    private Shipping() {}

    static double rate(double weightKg) {
        if (weightKg < 0) {
            throw new IllegalArgumentException("weight must not be negative: " + weightKg);
        }
        return weightKg <= 5 ? 5.0 + 2.0 * weightKg : 9.0 + 2.0 * weightKg;
    }
}
