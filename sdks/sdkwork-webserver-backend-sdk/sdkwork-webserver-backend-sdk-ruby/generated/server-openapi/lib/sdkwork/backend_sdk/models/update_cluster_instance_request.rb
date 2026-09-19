module Sdkwork
  module BackendSdk
    module Models
      class UpdateClusterInstanceRequest
              attr_accessor :name, :status, :public_endpoint

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @status = attributes['status']
                @public_endpoint = attributes['publicEndpoint']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'status' => @status,
                  'publicEndpoint' => @public_endpoint,
                }
              end
            end
    end
  end
end
