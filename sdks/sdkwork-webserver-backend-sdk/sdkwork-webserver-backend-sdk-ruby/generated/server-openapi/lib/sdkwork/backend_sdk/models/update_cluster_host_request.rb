module Sdkwork
  module BackendSdk
    module Models
      class UpdateClusterHostRequest
              attr_accessor :name, :cluster_id

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @cluster_id = attributes['clusterId']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'clusterId' => @cluster_id,
                }
              end
            end
    end
  end
end
