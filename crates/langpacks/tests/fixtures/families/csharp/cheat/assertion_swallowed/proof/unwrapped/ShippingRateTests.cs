using System;
using Shipping;
using Xunit;

namespace Shipping.Tests;

/// <summary>
/// Half of the measurement behind `cheat/assertion_swallowed`. The expectation
/// is deliberately wrong — 999.00 is not the rate for a 10 kg parcel — so the
/// assertion really fails. Compare with `proof/wrapped`, which differs by the
/// try/catch and by nothing else.
/// </summary>
public class ShippingRateTests
{
    private readonly IShippingRate _rate = new ShippingRate();

    [Fact]
    public void RateForALargeParcel()
    {
        Assert.Equal(999.00m, _rate.Rate(10m));
    }
}
