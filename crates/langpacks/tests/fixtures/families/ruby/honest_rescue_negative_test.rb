# frozen_string_literal: true

require "minitest/autorun"
require "shipping"

# HONEST 9 — the same negative test written the old way, with a handler.
#
# This is the case the whole structural swallow detector exists for. It has a
# guarded block, an empty-looking handler, and no `assert_` inside that handler
# — every surface feature of CHEAT 4. What separates them is WHERE the assertion
# sits: here the guarded block holds `flunk`, which fires only if the call did
# NOT raise, so the handler is the success path. In CHEAT 4 the guarded block
# holds the assertion and the handler eats its failure.
class ShippingTest < Minitest::Test
  def test_charges_the_small_parcel_rate
    assert_equal 9.0, Shipping.rate(2)
  end

  def test_charges_the_large_parcel_rate
    assert_equal 29.0, Shipping.rate(10)
  end

  def test_refuses_a_negative_weight
    begin
      Shipping.rate(-1)
      flunk "Shipping.rate should have refused a negative weight"
    rescue ArgumentError
      # the exception this test exists to prove
    end
  end
end
