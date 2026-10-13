# frozen_string_literal: true

require "minitest/autorun"
require "shipping"

# The base tree for the minitest half of the Ruby families.
#
# minitest is here and not only RSpec because `begin` / `rescue` has no RSpec
# spelling, and `begin` / `rescue` is the one shape where the honest negative
# test and the swallowed assertion are made of the same keywords.
class ShippingTest < Minitest::Test
  def test_charges_the_small_parcel_rate
    assert_equal 9.0, Shipping.rate(2)
  end

  def test_charges_the_large_parcel_rate
    assert_equal 29.0, Shipping.rate(10)
  end

  def test_refuses_a_negative_weight
    assert_nil Shipping.rate(-1)
  end
end
