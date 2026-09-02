module Sdkwork
  module BackendSdk
    module Models
      class CreateRootDomainRequest
              attr_accessor :hostname

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @hostname = attributes['hostname']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'hostname' => @hostname,
                }
              end
            end
    end
  end
end
