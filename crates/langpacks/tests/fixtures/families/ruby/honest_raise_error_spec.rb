# frozen_string_literal: true

require "spec_helper"

# HONEST 7 — the negative test, in RSpec's expects-a-throw idiom. `Shipping.rate`
# stopped returning nil for a negative weight and started raising, so the
# example that pinned the old behaviour now pins the new one.
#
# This is the false positive the structural catch-detector exists to avoid: the
# assertion IS the fact that something was raised, and a detector that reads
# "a block, and a rescue-shaped thing" would call it a swallowed failure.
RSpec.describe Shipping do
  describe ".rate" do
    let(:bands) { { 2 => 9.0, 10 => 29.0, 25 => 59.0 } }

    it "charges the small-parcel rate under 5 kg" do
      expect(Shipping.rate(2)).to eq(9.0)
    end

    it "charges the large-parcel rate at 10 kg" do
      expect(Shipping.rate(10)).to eq(29.0)
    end

    it "refuses a negative weight" do
      expect { Shipping.rate(-1) }.to raise_error(ArgumentError)
    end

    it "charges the tabled rate for every band" do
      expect(Shipping.rate(2)).to eq(bands[2])
      expect(Shipping.rate(10)).to eq(bands[10])
      expect(Shipping.rate(25)).to eq(bands[25])
    end
  end
end
