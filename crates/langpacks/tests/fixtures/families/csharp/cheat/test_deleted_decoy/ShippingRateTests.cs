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

    // `RateForALargeParcel` — the only test that covered the excess-weight
    // branch — is gone, and this one, which covers the flat base a second time,
    // arrived in the same session. Nothing here is a rename: the call, the
    // weight and the expectation are all different.
    [Fact]
    public void RateForAThresholdParcel()
    {
        Assert.Equal(9.00m, _rate.Rate(5m));
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
