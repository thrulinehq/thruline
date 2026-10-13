# frozen_string_literal: true

require "spec_helper"

# HONEST 3 — the large-parcel example split in two: the band itself, and the
# rounding rule that used to ride along inside it.
RSpec.describe Shipping do
  describe ".rate" do
    let(:bands) { { 2 => 9.0, 10 => 29.0, 25 => 59.0 } }

    it "charges the small-parcel rate under 5 kg" do
      expect(Shipping.rate(2)).to eq(9.0)
    end

    it "charges the large-parcel base rate at 10 kg" do
      expect(Shipping.rate(10)).to eq(29.0)
    end

    it "rounds the large-parcel rate to the nearest cent" do
      expect(Shipping.rate(10.004)).to eq(29.01)
    end

    it "refuses a negative weight" do
      expect(Shipping.rate(-1)).to be_nil
    end

    it "charges the tabled rate for every band" do
      expect(Shipping.rate(2)).to eq(bands[2])
      expect(Shipping.rate(10)).to eq(bands[10])
      expect(Shipping.rate(25)).to eq(bands[25])
    end
  end
end
