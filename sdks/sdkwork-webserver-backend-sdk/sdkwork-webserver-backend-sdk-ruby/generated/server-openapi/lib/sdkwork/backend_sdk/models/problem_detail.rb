module Sdkwork
  module BackendSdk
    module Models
      class ProblemDetail
              attr_accessor :type, :title, :status, :detail, :instance, :code, :trace_id, :errors

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @type = attributes['type']
                @title = attributes['title']
                @status = attributes['status']
                @detail = attributes['detail']
                @instance = attributes['instance']
                @code = attributes['code']
                @trace_id = attributes['traceId']
                @errors = attributes['errors'].is_a?(Array) ? attributes['errors'].map { |item| item.is_a?(Hash) ? FieldError.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'type' => @type,
                  'title' => @title,
                  'status' => @status,
                  'detail' => @detail,
                  'instance' => @instance,
                  'code' => @code,
                  'traceId' => @trace_id,
                  'errors' => @errors.is_a?(Array) ? @errors.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
