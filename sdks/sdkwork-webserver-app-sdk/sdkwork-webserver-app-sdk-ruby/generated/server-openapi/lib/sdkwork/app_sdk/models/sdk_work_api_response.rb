module Sdkwork
  module AppSdk
    module Models
      class SdkWorkApiResponse
              attr_accessor :code, :data, :trace_id

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @code = attributes['code']
                @data = attributes['data']
                @trace_id = attributes['traceId']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'code' => @code,
                  'data' => @data,
                  'traceId' => @trace_id,
                }
              end
            end
    end
  end
end
