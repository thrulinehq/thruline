# frozen_string_literal: true

require "spec_helper"

# HONEST 4 — the control. A tariff change moves the expected figures and
# nothing else; this is the one honest shape that trips no detector at all, and
# it is here so a green suite cannot be a suite that only ever sees this case.
RSpec.describe Shipping do
  describe ".rate" do
    let(:bands) { { 2 => 12.5, 10 => 32.5, 25 => 59.0 } }

    it "charges the small-parcel rate under 5 kg" do
      expect(Shipping.rate(2)).to eq(12.5)
    end

    it "charges the large-parcel rate at 10 kg" do
      expect(Shipping.rate(10)).to eq(32.5)
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
