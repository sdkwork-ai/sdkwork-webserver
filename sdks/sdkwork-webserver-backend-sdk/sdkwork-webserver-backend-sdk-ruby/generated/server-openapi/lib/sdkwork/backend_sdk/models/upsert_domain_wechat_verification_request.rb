module Sdkwork
  module BackendSdk
    module Models
      class UpsertDomainWechatVerificationRequest
              attr_accessor :file_name, :content

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @file_name = attributes['fileName']
                @content = attributes['content']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'fileName' => @file_name,
                  'content' => @content,
                }
              end
            end
    end
  end
end
