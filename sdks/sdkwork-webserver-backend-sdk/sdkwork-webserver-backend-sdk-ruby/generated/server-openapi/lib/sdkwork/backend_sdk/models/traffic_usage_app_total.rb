module Sdkwork
  module BackendSdk
    module Models
      class TrafficUsageAppTotal
              # One app's aggregate over the window. A row carrying neither `appUuid` nor `appSlug` is the **unattributed** bucket: traffic served for a hostname the edge could not resolve to an app. It is reported rather than dropped so the per-app rows keep summing back to the total; a surface must render it as its own row, or the breakdown appears to lose traffic.
              attr_accessor :app_uuid, :app_slug, :dimension, :quantity, :unit

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @app_uuid = attributes['appUuid']
                @app_slug = attributes['appSlug']
                @dimension = attributes['dimension']
                @quantity = attributes['quantity']
                @unit = attributes['unit']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'appUuid' => @app_uuid,
                  'appSlug' => @app_slug,
                  'dimension' => @dimension,
                  'quantity' => @quantity,
                  'unit' => @unit,
                }
              end
            end
    end
  end
end
