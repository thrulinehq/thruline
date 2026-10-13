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
        Assert.Equal(34.00m, _rate.Rate(10m));
    }

    [Fact]
    public void RateIsChargedPerKilogramAboveTheThreshold()
    {
        Assert.Equal(14.00m, _rate.Rate(6m));
        Assert.Equal(19.00m, _rate.Rate(7m));
    }

    [Fact]
    public void RateRejectsAWeightlessParcel()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => _rate.Rate(0m));
    }
}
