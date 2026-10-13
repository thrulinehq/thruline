using System;
using Shipping;
using Xunit;

namespace Shipping.Tests;

public class ShippingRateTests
{
    private readonly IShippingRate _rate = new ShippingRate();

    /// <summary>
    /// The predicate xunit v3 evaluates for <c>SkipWhen</c>. This capture ran on
    /// macOS, so it is false and the test below actually ran.
    /// </summary>
    public static bool RunningOnWindows => OperatingSystem.IsWindows();

    [Fact]
    public void RateForASmallParcel()
    {
        Assert.Equal(9.00m, _rate.Rate(2m));
    }

    [Fact(Skip = "decimal rounding differs under the Windows CRT", SkipWhen = nameof(RunningOnWindows))]
    public void RateForALargeParcel()
    {
        Assert.Equal(29.00m, _rate.Rate(10m));
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
