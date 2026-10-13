# frozen_string_literal: true

require "spec_helper"

# HONEST 1 — two examples renamed, both bodies byte-identical to the base.
RSpec.describe Shipping do
  describe ".rate" do
    let(:bands) { { 2 => 9.0, 10 => 29.0, 25 => 59.0 } }

    it "charges 9.00 for a parcel under the 5 kg band" do
      expect(Shipping.rate(2)).to eq(9.0)
    end

    it "charges 29.00 for a parcel in the 10 kg band" do
      expect(Shipping.rate(10)).to eq(29.0)
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
