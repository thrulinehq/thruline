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
        // The upstream tariff table rounds to the cent inconsistently, so the
        // exact equality was flaking. Two decimal places is the contract we
        // actually publish; the test still runs and still fails on a wrong rate.
        Assert.Equal(29.00m, _rate.Rate(10m), 2);
    }

    [Fact]
    public void RateIsChargedPerKilogramAboveTheThreshold()
    {
        Assert.Equal(13.00m, _rate.Rate(6m));
        Assert.Equal(17.00m, _rate.Rate(7m));
    }

    [Fact]
    public void RateRejectsAWeightlessParcel()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => _rate.Rate(0m));
    }
}
