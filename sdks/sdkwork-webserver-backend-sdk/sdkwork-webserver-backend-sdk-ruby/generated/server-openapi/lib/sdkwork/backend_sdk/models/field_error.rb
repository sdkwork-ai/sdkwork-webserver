module Sdkwork
  module BackendSdk
    module Models
      class FieldError
              attr_accessor :field, :message, :code

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @field = attributes['field']
                @message = attributes['message']
                @code = attributes['code']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'field' => @field,
                  'message' => @message,
                  'code' => @code,
                }
              end
            end
    end
  end
end
