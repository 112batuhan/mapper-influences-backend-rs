ARG SURREAL_USER
ARG SURREAL_PASS
ARG SURREAL_URL

ARG CLIENT_ID
ARG CLIENT_SECRET
ARG REDIRECT_URI
ARG POST_LOGIN_REDIRECT_URI

ARG JWT_SECRET_KEY

ARG PORT

FROM rust:latest as rust-builder
WORKDIR /usr/src/mapper_influences_backend
COPY . .
RUN cargo build --release

FROM rust:slim
WORKDIR /usr/src/mapper_influences_backend
COPY --from=rust-builder /usr/src/mapper_influences_backend/target/release/mapper-influences-backend .
COPY --from=rust-builder /usr/src/mapper_influences_backend/src/elements-ui.html .

ENV SURREAL_USER=${SURREAL_USER}
ARG SURREAL_PASS=${SURREAL_PASS}
ARG SURREAL_URL=${SURREAL_URL}

ARG CLIENT_ID=${SURREAL_USER}
ARG CLIENT_SECRET=${CLIENT_SECRET}
ARG REDIRECT_URI=${REDIRECT_URI}
ARG POST_LOGIN_REDIRECT_URI=${POST_LOGIN_REDIRECT_URI}

ARG JWT_SECRET_KEY=${JWT_SECRET_KEY}

ARG PORT=${PORT}

EXPOSE ${PORT}
ENTRYPOINT ["./mapper-influences-backend"]
