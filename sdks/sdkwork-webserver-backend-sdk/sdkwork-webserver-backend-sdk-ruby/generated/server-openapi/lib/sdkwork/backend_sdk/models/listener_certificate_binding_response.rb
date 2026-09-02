module Sdkwork
  module BackendSdk
    module Models
      class ListenerCertificateBindingResponse
              attr_accessor :id, :site_id, :domain_id, :certificate_id, :desired_certificate_version_id, :current_certificate_version_id, :desired_certificate, :current_certificate, :key_algorithm, :priority, :is_default, :status, :activated_at, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @site_id = attributes['siteId']
                @domain_id = attributes['domainId']
                @certificate_id = attributes['certificateId']
                @desired_certificate_version_id = attributes['desiredCertificateVersionId']
                @current_certificate_version_id = attributes['currentCertificateVersionId']
                @desired_certificate = attributes['desiredCertificate'].is_a?(Hash) ? ListenerCertificateSummaryResponse.from_hash(attributes['desiredCertificate']) : nil
                @current_certificate = attributes['currentCertificate'].is_a?(Hash) ? ListenerCertificateSummaryResponse.from_hash(attributes['currentCertificate']) : nil
                @key_algorithm = attributes['keyAlgorithm']
                @priority = attributes['priority']
                @is_default = attributes['isDefault']
                @status = attributes['status']
                @activated_at = attributes['activatedAt']
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'siteId' => @site_id,
                  'domainId' => @domain_id,
                  'certificateId' => @certificate_id,
                  'desiredCertificateVersionId' => @desired_certificate_version_id,
                  'currentCertificateVersionId' => @current_certificate_version_id,
                  'desiredCertificate' => @desired_certificate&.to_hash,
                  'currentCertificate' => @current_certificate&.to_hash,
                  'keyAlgorithm' => @key_algorithm,
                  'priority' => @priority,
                  'isDefault' => @is_default,
                  'status' => @status,
                  'activatedAt' => @activated_at,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end
