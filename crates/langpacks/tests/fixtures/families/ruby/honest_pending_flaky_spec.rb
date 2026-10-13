# frozen_string_literal: true

require "spec_helper"

# HONEST 5 — a known-flaky example marked expected-to-fail. RSpec's `pending`
# still RUNS the example and requires it to fail, which is why it reports as a
# pending example rather than as a skipped one; the run below says 1 pending, 0
# failures.
RSpec.describe Shipping do
  describe ".rate" do
    let(:bands) { { 2 => 9.0, 10 => 29.0, 25 => 59.0 } }

    it "charges the small-parcel rate under 5 kg" do
      expect(Shipping.rate(2)).to eq(9.0)
    end

    it "charges the large-parcel rate at 10 kg" do
      pending("upstream rounding is unstable above 8 kg")
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
