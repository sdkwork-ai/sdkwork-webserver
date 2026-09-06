module Sdkwork
  module BackendSdk
    module Models
      class WebserverConfigWriteRequest
              attr_accessor :content, :expected_sha256

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @content = attributes['content']
                @expected_sha256 = attributes['expectedSha256']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'content' => @content,
                  'expectedSha256' => @expected_sha256,
                }
              end
            end
    end
  end
end
