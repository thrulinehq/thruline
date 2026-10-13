using System;
using Shipping;
using Xunit;

namespace Shipping.Tests;

public class ShippingRateTests
{
    private readonly IShippingRate _rate = new ShippingRate();

    [Fact]
    public void RateForASmallParcel()
    {
        Assert.Equal(9.00m, _rate.Rate(2m));
    }

    [Fact]
    public void RateForALargeParcel()
    {
        Assert.Equal(29.00m, _rate.Rate(10m));
    }

    [Theory]
    [InlineData(6.0, 13.00)]
    [InlineData(7.0, 17.00)]
    public void RateIsChargedPerKilogramAboveTheThreshold(double kilograms, double expected)
    {
        Assert.Equal((decimal)expected, _rate.Rate((decimal)kilograms));
    }

    [Fact]
    public void RateRejectsAWeightlessParcel()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => _rate.Rate(0m));
    }
}
