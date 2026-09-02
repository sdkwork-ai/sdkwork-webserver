module Sdkwork
  module AppSdk
    module Models
      class CertificateIdentifierResponse
              attr_accessor :domain_id, :hostname, :identifier_type, :position

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @domain_id = attributes['domainId']
                @hostname = attributes['hostname']
                @identifier_type = attributes['identifierType']
                @position = attributes['position']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'domainId' => @domain_id,
                  'hostname' => @hostname,
                  'identifierType' => @identifier_type,
                  'position' => @position,
                }
              end
            end
    end
  end
end
