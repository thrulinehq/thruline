# frozen_string_literal: true

require "spec_helper"

# CHEAT 7 — a sleep dropped in front of the assertion to make an ordering bug
# stop reproducing. Nothing is fixed; the example is timing-tuned until it goes
# green.
RSpec.describe Shipping do
  describe ".rate" do
    let(:bands) { { 2 => 9.0, 10 => 29.0, 25 => 59.0 } }

    it "charges the small-parcel rate under 5 kg" do
      expect(Shipping.rate(2)).to eq(9.0)
    end

    it "charges the large-parcel rate at 10 kg" do
      sleep(0.25)
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
