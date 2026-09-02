module Sdkwork
  module BackendSdk
    module Models
      class NginxValidateResponse
              attr_accessor :valid, :errors

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @valid = attributes['valid']
                @errors = attributes['errors'].is_a?(Array) ? attributes['errors'].map { |item| item.is_a?(Hash) ? item : {} } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'valid' => @valid,
                  'errors' => @errors.is_a?(Array) ? @errors.map { |item| item } : [],
                }
              end
            end
    end
  end
end
