module Sdkwork
  module BackendSdk
    module Models
      class MetricsWindowBounds
              attr_accessor :window, :date_from, :date_to

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @window = attributes['window']
                @date_from = attributes['dateFrom']
                @date_to = attributes['dateTo']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'window' => @window,
                  'dateFrom' => @date_from,
                  'dateTo' => @date_to,
                }
              end
            end
    end
  end
end
