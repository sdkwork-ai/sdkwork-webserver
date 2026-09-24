module Sdkwork
  module BackendSdk
    module Models
      class MetricsSeriesWindow
              # The window the per-day series was cut against, as the server resolved it. Reported because the request's bounds are optional: a surface that labelled the series from its own guess at the default would name a period the points do not cover. Deliberately separate from the card windows, which answer a different question and may not coincide with this one.
              attr_accessor :date_from, :date_to

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @date_from = attributes['dateFrom']
                @date_to = attributes['dateTo']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'dateFrom' => @date_from,
                  'dateTo' => @date_to,
                }
              end
            end
    end
  end
end
