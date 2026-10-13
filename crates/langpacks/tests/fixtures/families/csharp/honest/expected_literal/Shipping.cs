namespace Shipping;

/// <summary>The unit under test, behind the interface the mock case fakes.</summary>
public interface IShippingRate
{
    decimal Rate(decimal kilograms);
}

public sealed class ShippingRate : IShippingRate
{
    public const decimal FlatBase = 9.00m;
    public const decimal ThresholdKg = 5.00m;

    // The 2026 tariff: the per-kilogram surcharge above the threshold went
    // from 4.00 to 5.00. The expectations in ShippingRateTests.cs move with it.
    public const decimal PerKilogram = 5.00m;

    public decimal Rate(decimal kilograms)
    {
        if (kilograms <= 0m)
        {
            throw new ArgumentOutOfRangeException(nameof(kilograms), "a parcel weighs something");
        }

        return kilograms <= ThresholdKg
            ? FlatBase
            : FlatBase + (kilograms - ThresholdKg) * PerKilogram;
    }
}
