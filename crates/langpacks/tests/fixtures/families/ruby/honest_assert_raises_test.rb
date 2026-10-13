# frozen_string_literal: true

require "minitest/autorun"
require "shipping"

# HONEST 8 — the negative test in minitest's own expects-a-throw idiom.
# `assert_raises` takes the block, so the guarded call and the assertion are one
# expression and there is no handler at all.
class ShippingTest < Minitest::Test
  def test_charges_the_small_parcel_rate
    assert_equal 9.0, Shipping.rate(2)
  end

  def test_charges_the_large_parcel_rate
    assert_equal 29.0, Shipping.rate(10)
  end

  def test_refuses_a_negative_weight
    assert_raises(ArgumentError) { Shipping.rate(-1) }
  end
end
