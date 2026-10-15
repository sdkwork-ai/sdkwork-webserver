module Sdkwork
  module BackendSdk
    module Models
      class TunnelMachineRouteDeclarationResponse
              attr_accessor :route_id, :active, :public_url, :detail

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @route_id = attributes['routeId']
                @active = attributes['active']
                @public_url = attributes['publicUrl']
                @detail = attributes['detail']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'routeId' => @route_id,
                  'active' => @active,
                  'publicUrl' => @public_url,
                  'detail' => @detail,
                }
              end
            end
    end
  end
end
