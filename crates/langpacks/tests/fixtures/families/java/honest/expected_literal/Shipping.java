package demo;

/** The unit under test. Two bands, and a rejection for a negative weight. */
final class Shipping {

    private Shipping() {}

    static double rate(double weightKg) {
        if (weightKg < 0) {
            throw new IllegalArgumentException("weight must not be negative: " + weightKg);
        }
        return weightKg <= 5 ? 6.0 + 2.5 * weightKg : 12.0 + 2.5 * weightKg;
    }
}
