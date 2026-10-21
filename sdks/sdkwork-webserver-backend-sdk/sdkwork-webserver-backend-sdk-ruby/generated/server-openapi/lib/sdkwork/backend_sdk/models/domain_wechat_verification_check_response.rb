module Sdkwork
  module BackendSdk
    module Models
      class DomainWechatVerificationCheckResponse
              attr_accessor :reachable, :matched, :status_code, :scheme, :detail, :checked_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @reachable = attributes['reachable']
                @matched = attributes['matched']
                @status_code = attributes['statusCode']
                @scheme = attributes['scheme']
                @detail = attributes['detail']
                @checked_at = attributes['checkedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'reachable' => @reachable,
                  'matched' => @matched,
                  'statusCode' => @status_code,
                  'scheme' => @scheme,
                  'detail' => @detail,
                  'checkedAt' => @checked_at,
                }
              end
            end
    end
  end
end
