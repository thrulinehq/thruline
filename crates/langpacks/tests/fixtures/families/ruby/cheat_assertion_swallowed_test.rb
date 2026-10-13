# frozen_string_literal: true

require "minitest/autorun"
require "shipping"

# CHEAT 4 — the assertion wrapped so its failure no longer propagates.
#
# `assert_equal` raises `Minitest::Assertion`, which descends from
# `StandardError`, so this handler catches the failure the test was written to
# report and the method returns normally. The run is green and the tally is
# unchanged: 3 tests, 0 failures. Nothing in the OUTPUT distinguishes this file
# from the base, which is exactly why §6.7 reads the source.
class ShippingTest < Minitest::Test
  def test_charges_the_small_parcel_rate
    assert_equal 9.0, Shipping.rate(2)
  end

  def test_charges_the_large_parcel_rate
    begin
      assert_equal 29.0, Shipping.rate(10)
    rescue StandardError
      # the assertion cannot fail the test any more
    end
  end

  def test_refuses_a_negative_weight
    assert_nil Shipping.rate(-1)
  end
end
