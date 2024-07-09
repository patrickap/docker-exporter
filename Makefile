IMAGE_NAME=patrickap/docker-exporter
IMAGE_VERSION="$$(cat VERSION)"

release-patch:
	$(call increment-version,patch)
	$(MAKE) docker-build
	$(MAKE) docker-publish
	$(MAKE) git-publish

release-minor:
	$(call increment-version,minor)
	$(MAKE) docker-build
	$(MAKE) docker-publish
	$(MAKE) git-publish

release-major:
	$(call increment-version,major)
	$(MAKE) docker-build
	$(MAKE) docker-publish
	$(MAKE) git-publish

docker-build:
	docker build --no-cache -t $(IMAGE_NAME):$(IMAGE_VERSION) .

docker-publish:
	docker push $(IMAGE_NAME):$(IMAGE_VERSION)

git-publish:
	git add .
	git commit -m "chore(release): $(IMAGE_VERSION)"
	git push

define increment-version
	MAJOR=$$(echo $(IMAGE_VERSION) | cut -d'.' -f1); \
	MINOR=$$(echo $(IMAGE_VERSION) | cut -d'.' -f2); \
	PATCH=$$(echo $(IMAGE_VERSION) | cut -d'.' -f3); \
	if [ "$1" = "major" ]; then \
		MAJOR=$$((MAJOR + 1)); \
		MINOR=0; \
		PATCH=0; \
	elif [ "$1" = "minor" ]; then \
		MINOR=$$((MINOR + 1)); \
		PATCH=0; \
	elif [ "$1" = "patch" ]; then \
		PATCH=$$((PATCH + 1)); \
	fi; \
	NEW_VERSION=$$MAJOR.$$MINOR.$$PATCH; \
	echo "$$NEW_VERSION" > VERSION
endef
