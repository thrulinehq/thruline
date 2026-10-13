# frozen_string_literal: true

require "spec_helper"

# HONEST 6 — the three hand-written band assertions folded into the table they
# were already reading from. The example keeps its name and covers exactly what
# it covered before, and its assertion count drops from three to one.
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
      expect(Shipping.rate(-1)).to be_nil
    end

    it "charges the tabled rate for every band" do
      bands.each { |weight, expected| expect(Shipping.rate(weight)).to eq(expected) }
    end
  end
end
