using System;
using Shipping;
using Xunit;

namespace Shipping.Tests;

public class ShippingRateTests
{
    private readonly IShippingRate _rate = new ShippingRate();

    [Fact]
    public void ASmallParcelIsChargedTheFlatBase()
    {
        Assert.Equal(9.00m, _rate.Rate(2m));
    }

    [Fact]
    public void ALargeParcelIsChargedTheFlatBasePlusExcessWeight()
    {
        Assert.Equal(29.00m, _rate.Rate(10m));
    }

    [Fact]
    public void EachKilogramAboveTheThresholdAddsToTheRate()
    {
        Assert.Equal(13.00m, _rate.Rate(6m));
        Assert.Equal(17.00m, _rate.Rate(7m));
    }

    [Fact]
    public void AWeightlessParcelHasNoRate()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => _rate.Rate(0m));
    }
}
