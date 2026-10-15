module Sdkwork
  module BackendSdk
    module Models
      class TunnelMachineListResponse
              attr_accessor :machines

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @machines = attributes['machines'].is_a?(Array) ? attributes['machines'].map { |item| item.is_a?(Hash) ? TunnelMachineResponse.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'machines' => @machines.is_a?(Array) ? @machines.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
