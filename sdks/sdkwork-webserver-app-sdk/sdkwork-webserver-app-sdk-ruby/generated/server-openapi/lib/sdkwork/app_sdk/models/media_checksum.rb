module Sdkwork
  module AppSdk
    module Models
      class MediaChecksum
              attr_accessor :algorithm, :value

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @algorithm = attributes['algorithm']
                @value = attributes['value']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'algorithm' => @algorithm,
                  'value' => @value,
                }
              end
            end
    end
  end
end
