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

    [Fact]
    public void RateChargesForTheFirstKilogramAboveTheThreshold()
    {
        Assert.Equal(13.00m, _rate.Rate(6m));
    }

    [Fact]
    public void RateChargesForEveryFurtherKilogram()
    {
        Assert.Equal(17.00m, _rate.Rate(7m));
    }

    [Fact]
    public void RateRejectsAWeightlessParcel()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => _rate.Rate(0m));
    }
}
