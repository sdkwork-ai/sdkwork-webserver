module Sdkwork
  module AppSdk
    module Models
      class SdkWorkResourceData
              attr_accessor :item

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @item = attributes['item'].is_a?(Hash) ? attributes['item'] : {}
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'item' => @item,
                }
              end
            end
    end
  end
end
